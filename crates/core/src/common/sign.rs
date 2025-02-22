use crate::traits::RSASHA2SigningKey;
use axum::http::Request;
use axum::http::header::{DATE, HOST};
use base64::Engine;
use bytes::Bytes;
use chrono::{DateTime, Utc};
use digest::{FixedOutput, Update};
use ring_compat::digest::Sha256;
use ring_compat::signature::{SignatureEncoding, Signer};

pub fn sign(mut request: Request<Bytes>, key_id: &str, key: &RSASHA2SigningKey, date: DateTime<Utc>) -> Request<Bytes> {
    if request.headers().get(HOST).is_none() {
        let uri = request.uri();
        let host = uri.authority().unwrap().as_str().to_owned();
        request.headers_mut().insert(HOST, host.parse().unwrap());
    }
    if request.headers().get(DATE).is_none() {
        let date = date.format("%a, %d %b %Y %H:%M:%S GMT").to_string();
        request.headers_mut().insert(DATE, date.parse().unwrap());
    }

    let body_digest = {
        let mut hasher = Sha256::default();
        hasher.update(request.body());
        hasher.finalize_fixed()
    };
    let body_digest = base64::engine::general_purpose::STANDARD.encode(body_digest);
    request
        .headers_mut()
        .insert("digest", format!("SHA-256={}", body_digest).parse().unwrap());

    let sign_target = format!(
        "(request-target): {} {}\ndate: {}\nhost: {}\ndigest: {}",
        request.method().as_str().to_lowercase(),
        request.uri().path_and_query().unwrap().as_str(),
        request.headers().get(DATE).unwrap().to_str().unwrap(),
        request.headers().get(HOST).unwrap().to_str().unwrap(),
        request.headers().get("digest").unwrap().to_str().unwrap(),
    );
    let signature = key.sign(sign_target.as_bytes());
    let signature = base64::engine::general_purpose::STANDARD.encode(signature.to_bytes());
    request.headers_mut().insert(
        "signature",
        format!(
            "keyId=\"{}\",algorithm=\"rsa-sha256\",headers=\"(request-target) date host digest\",signature=\"{}\"",
            key_id, signature
        )
        .parse()
        .unwrap(),
    );
    request
}
