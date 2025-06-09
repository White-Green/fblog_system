use crate::traits::HTTPClient;
use axum::http::Request;
use base64::Engine;
use bytes::{Bytes, BytesMut};
use http_body_util::{BodyExt, Limited};
use rsa::pkcs1v15::{Signature, VerifyingKey};
use rsa::pkcs8::DecodePublicKey;
use rsa::sha2::{Digest, Sha256};
use rsa::signature::Verifier;
use rsa::RsaPublicKey;
use serde::Deserialize;

#[derive(Debug)]
pub enum VerifiedRequest<B> {
    Verified(Request<VerifyBody<B>>),
    CannotVerify(Request<Limited<B>>),
    VerifyFailed,
}

const BODY_LIMIT: usize = 1024 * 64;

#[derive(Debug)]
pub struct VerifyBody<B> {
    inner: Limited<B>,
    hasher: Option<Sha256>,
    expected_digest: Option<String>,
}

impl<B> VerifyBody<B>
where
    B: http_body::Body<Data = Bytes> + Unpin,
    B::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
{
    pub async fn collect_to_bytes(mut self) -> Result<(Bytes, bool), Box<dyn std::error::Error + Send + Sync>> {
        let mut out = BytesMut::new();
        while let Some(frame) = self.inner.frame().await {
            let frame = frame?;
            if let Some(data) = frame.data_ref() {
                if let Some(ref mut hasher) = self.hasher {
                    hasher.update(data);
                }
                out.extend_from_slice(data);
            }
        }
        let digest_ok = if let (Some(hasher), Some(expected)) = (self.hasher.take(), self.expected_digest.take()) {
            let digest = base64::engine::general_purpose::STANDARD.encode(hasher.finalize());
            format!("SHA-256={}", digest) == expected
        } else {
            true
        };
        Ok((out.freeze(), digest_ok))
    }
}

#[tracing::instrument(skip(state, req))]
pub async fn verify_request<E, B>(state: &E, req: Request<B>) -> VerifiedRequest<B>
where
    E: HTTPClient,
    B: http_body::Body<Data = Bytes> + Unpin,
    B::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
{
    let (mut parts, body) = req.into_parts();
    let method = parts.method.clone();
    let path = parts
        .uri
        .path_and_query()
        .map(|pq| pq.as_str())
        .unwrap_or(parts.uri.path());
    let headers = &parts.headers;

    let signature_header = headers
        .get("Signature")
        .or_else(|| headers.get("signature"))
        .and_then(|v| v.to_str().ok());
    let Some(signature_header) = signature_header else {
        tracing::warn!("missing signature header");
        return VerifiedRequest::CannotVerify(Request::from_parts(parts, Limited::new(body, BODY_LIMIT)));
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
        return VerifiedRequest::CannotVerify(Request::from_parts(parts, Limited::new(body, BODY_LIMIT)));
    };
    let Some(algorithm) = algorithm else {
        tracing::warn!("missing algorithm");
        return VerifiedRequest::CannotVerify(Request::from_parts(parts, Limited::new(body, BODY_LIMIT)));
    };
    let Some(signed_headers) = signed_headers else {
        tracing::warn!("missing signed headers");
        return VerifiedRequest::CannotVerify(Request::from_parts(parts, Limited::new(body, BODY_LIMIT)));
    };
    let Some(signature) = signature else {
        tracing::warn!("missing signature");
        return VerifiedRequest::CannotVerify(Request::from_parts(parts, Limited::new(body, BODY_LIMIT)));
    };

    if algorithm != "rsa-sha256" {
        tracing::warn!(algo = %algorithm, "unsupported algorithm");
        return VerifiedRequest::CannotVerify(Request::from_parts(parts, Limited::new(body, BODY_LIMIT)));
    }

    let mut sign_target = String::new();
    let mut first = true;
    let mut digest_header = None;
    for header_name in signed_headers.split_whitespace() {
        if !first {
            sign_target.push('\n');
        }
        first = false;

        if header_name.eq_ignore_ascii_case("(request-target)") {
            sign_target.push_str("(request-target): ");
            sign_target.push_str(&format!("{} {}", method.as_str().to_lowercase(), path));
        } else {
            let name = match axum::http::header::HeaderName::from_bytes(header_name.as_bytes()) {
                Ok(n) => n,
                Err(_) => {
                    tracing::warn!(header = %header_name, "invalid header name");
                    return VerifiedRequest::CannotVerify(Request::from_parts(parts, Limited::new(body, BODY_LIMIT)));
                }
            };
            let value = match headers.get(&name).and_then(|v| v.to_str().ok()) {
                Some(v) => v,
                None => {
                    tracing::warn!(header = %header_name, "missing signed header");
                    return VerifiedRequest::CannotVerify(Request::from_parts(parts, Limited::new(body, BODY_LIMIT)));
                }
            };
            if header_name.eq_ignore_ascii_case("digest") {
                digest_header = Some(value.to_string());
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
    let request = match Request::get(actor_url)
        .header(axum::http::header::ACCEPT, crate::common::headers::AP_ACCEPT)
        .body(Bytes::new())
    {
        Ok(req) => req,
        Err(_) => {
            tracing::warn!("failed to build actor request");
            return VerifiedRequest::CannotVerify(Request::from_parts(parts, Limited::new(body, BODY_LIMIT)));
        }
    };
    let response = match state.request(request).await {
        Ok(resp) => resp,
        Err(_) => {
            tracing::warn!("failed to fetch actor");
            return VerifiedRequest::CannotVerify(Request::from_parts(parts, Limited::new(body, BODY_LIMIT)));
        }
    };
    if !response.status().is_success() {
        tracing::warn!(status = %response.status(), "actor fetch failed");
        return VerifiedRequest::CannotVerify(Request::from_parts(parts, Limited::new(body, BODY_LIMIT)));
    }
    let actor_body = match BodyExt::collect(response.into_body()).await {
        Ok(b) => b,
        Err(_) => {
            tracing::warn!("failed to read actor response");
            return VerifiedRequest::CannotVerify(Request::from_parts(parts, Limited::new(body, BODY_LIMIT)));
        }
    };
    let actor: Actor = match serde_json::from_slice(&actor_body.to_bytes()) {
        Ok(v) => v,
        Err(_) => {
            tracing::warn!("failed to parse actor");
            return VerifiedRequest::CannotVerify(Request::from_parts(parts, Limited::new(body, BODY_LIMIT)));
        }
    };
    let pem = match actor.key {
        Some(k) => k.pem,
        None => {
            tracing::warn!("actor has no public key");
            return VerifiedRequest::CannotVerify(Request::from_parts(parts, Limited::new(body, BODY_LIMIT)));
        }
    };

    let verifying_key = match RsaPublicKey::from_public_key_pem(&pem) {
        Ok(k) => VerifyingKey::<Sha256>::new(k),
        Err(_) => {
            tracing::warn!("invalid public key");
            return VerifiedRequest::CannotVerify(Request::from_parts(parts, Limited::new(body, BODY_LIMIT)));
        }
    };
    let sig_bytes = match base64::engine::general_purpose::STANDARD.decode(signature.as_bytes()) {
        Ok(b) => b,
        Err(_) => {
            tracing::warn!("invalid signature encoding");
            return VerifiedRequest::CannotVerify(Request::from_parts(parts, Limited::new(body, BODY_LIMIT)));
        }
    };
    let signature = match Signature::try_from(sig_bytes.as_slice()) {
        Ok(s) => s,
        Err(_) => {
            tracing::warn!("invalid signature format");
            return VerifiedRequest::CannotVerify(Request::from_parts(parts, Limited::new(body, BODY_LIMIT)));
        }
    };
    if verifying_key.verify(sign_target.as_bytes(), &signature).is_ok() {
        tracing::info!("signature verified");
        let body = VerifyBody {
            inner: Limited::new(body, BODY_LIMIT),
            hasher: digest_header.as_ref().map(|_| Sha256::new()),
            expected_digest: digest_header,
        };
        VerifiedRequest::Verified(Request::from_parts(parts, body))
    } else {
        tracing::info!("signature verification failed");
        VerifiedRequest::VerifyFailed
    }
}
