use crate::traits::HTTPClient;
use axum::http::HeaderMap;
use base64::Engine;
use bytes::Bytes;
use rsa::RsaPublicKey;
use rsa::pkcs1v15::{Signature, VerifyingKey};
use rsa::pkcs8::DecodePublicKey;
use rsa::sha2::{Digest, Sha256};
use rsa::signature::Verifier;
use serde::Deserialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerifyResult {
    Verified,
    CannotVerify,
    Failed,
}

#[tracing::instrument(skip(state, body))]
pub async fn verify_request<E>(state: &E, headers: &HeaderMap, method: &str, path: &str, body: &[u8]) -> VerifyResult
where
    E: HTTPClient,
{
    let Some(signature_header) = headers
        .get("Signature")
        .or_else(|| headers.get("signature"))
        .and_then(|v| v.to_str().ok())
    else {
        tracing::warn!("missing signature header");
        return VerifyResult::CannotVerify;
    };

    let mut key_id = None;
    let mut algorithm = None;
    let mut signed_headers = None;
    let mut signature = None;
    for part in signature_header.split(',') {
        let part = part.trim();
        let mut kv = part.splitn(2, '=');
        let k = kv.next().unwrap_or("");
        let v = kv.next().unwrap_or("").trim_matches('"');
        match k {
            "keyId" => key_id = Some(v.to_string()),
            "algorithm" => algorithm = Some(v.to_string()),
            "headers" => signed_headers = Some(v.to_string()),
            "signature" => signature = Some(v.to_string()),
            _ => {}
        }
    }
    let Some(key_id) = key_id else {
        tracing::warn!("missing keyId");
        return VerifyResult::CannotVerify;
    };
    let Some(algorithm) = algorithm else {
        tracing::warn!("missing algorithm");
        return VerifyResult::CannotVerify;
    };
    let Some(signed_headers) = signed_headers else {
        tracing::warn!("missing signed headers");
        return VerifyResult::CannotVerify;
    };
    let Some(signature) = signature else {
        tracing::warn!("missing signature");
        return VerifyResult::CannotVerify;
    };

    if algorithm != "rsa-sha256" {
        tracing::warn!(algo = %algorithm, "unsupported algorithm");
        return VerifyResult::CannotVerify;
    }
    let mut sign_target = String::new();
    let mut first = true;
    for header_name in signed_headers.split_whitespace() {
        if !first {
            sign_target.push('\n');
        }
        first = false;

        if header_name.eq_ignore_ascii_case("(request-target)") {
            sign_target.push_str("(request-target): ");
            sign_target.push_str(&format!("{} {}", method.to_lowercase(), path));
        } else {
            let name = match axum::http::header::HeaderName::from_bytes(header_name.as_bytes()) {
                Ok(n) => n,
                Err(_) => {
                    tracing::warn!(header = %header_name, "invalid header name");
                    return VerifyResult::CannotVerify;
                }
            };
            let value = match headers.get(&name).and_then(|v| v.to_str().ok()) {
                Some(v) => v,
                None => {
                    tracing::warn!(header = %header_name, "missing signed header");
                    return VerifyResult::CannotVerify;
                }
            };
            if header_name.eq_ignore_ascii_case("digest") {
                let computed_digest = {
                    let mut hasher = Sha256::new();
                    hasher.update(body);
                    let out = hasher.finalize();
                    base64::engine::general_purpose::STANDARD.encode(out)
                };
                if value != format!("SHA-256={}", computed_digest) {
                    tracing::warn!("digest mismatch");
                    return VerifyResult::Failed;
                }
            }
            sign_target.push_str(&format!("{}: {}", header_name.to_ascii_lowercase(), value));
        }
    }

    let actor_url = key_id.split('#').next().unwrap_or(&key_id);
    #[derive(Deserialize)]
    struct ActorKey {
        #[serde(rename = "publicKeyPem")]
        pem: String,
    }
    #[derive(Deserialize)]
    struct Actor {
        #[serde(rename = "publicKey")]
        key: Option<ActorKey>,
    }
    let request = match axum::http::Request::get(actor_url)
        .header(axum::http::header::ACCEPT, crate::common::headers::AP_ACCEPT)
        .body(Bytes::new())
    {
        Ok(req) => req,
        Err(_) => {
            tracing::warn!("failed to build actor request");
            return VerifyResult::CannotVerify;
        }
    };
    let response = match state.request(request).await {
        Ok(resp) => resp,
        Err(_) => {
            tracing::warn!("failed to fetch actor");
            return VerifyResult::CannotVerify;
        }
    };
    if !response.status().is_success() {
        tracing::warn!(status = %response.status(), "actor fetch failed");
        return VerifyResult::CannotVerify;
    }
    let body = match http_body_util::BodyExt::collect(response.into_body()).await {
        Ok(b) => b,
        Err(_) => {
            tracing::warn!("failed to read actor response");
            return VerifyResult::CannotVerify;
        }
    };
    let actor: Actor = match serde_json::from_slice(&body.to_bytes()) {
        Ok(v) => v,
        Err(_) => {
            tracing::warn!("failed to parse actor");
            return VerifyResult::CannotVerify;
        }
    };
    let pem = match actor.key {
        Some(k) => k.pem,
        None => {
            tracing::warn!("actor has no public key");
            return VerifyResult::CannotVerify;
        }
    };

    let verifying_key = match RsaPublicKey::from_public_key_pem(&pem) {
        Ok(k) => VerifyingKey::<Sha256>::new(k),
        Err(_) => {
            tracing::warn!("invalid public key");
            return VerifyResult::CannotVerify;
        }
    };
    let sig_bytes = match base64::engine::general_purpose::STANDARD.decode(signature.as_bytes()) {
        Ok(b) => b,
        Err(_) => {
            tracing::warn!("invalid signature encoding");
            return VerifyResult::CannotVerify;
        }
    };
    let signature = match Signature::try_from(sig_bytes.as_slice()) {
        Ok(s) => s,
        Err(_) => {
            tracing::warn!("invalid signature format");
            return VerifyResult::CannotVerify;
        }
    };
    if verifying_key
        .verify(sign_target.as_bytes(), &signature)
        .is_ok()
    {
        tracing::info!("signature verified");
        VerifyResult::Verified
    } else {
        tracing::info!("signature verification failed");
        VerifyResult::Failed
    }
}
