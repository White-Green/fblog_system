use crate::traits::{HTTPClient, Queue, QueueData, UserProvider};
use crate::verify::{VerifiedRequest, verify_request};
use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::header::CONTENT_TYPE;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use mime::Mime;
use serde::Deserialize;
use std::str::FromStr;

#[tracing::instrument(skip(state))]
pub async fn user_inbox_post<E>(header: HeaderMap, Path(username): Path<String>, State(state): State<E>, body: Body) -> Response<Body>
where
    E: UserProvider + Queue + HTTPClient,
{
    if !state.exists_user(&username).await {
        tracing::info!("user is not found");
        return StatusCode::NOT_FOUND.into_response();
    }
    if !header
        .get(CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| Mime::from_str(v).ok())
        .is_some_and(|ty| crate::common::headers::is_content_type_ap(&ty))
    {
        tracing::info!("invalid content type");
        return StatusCode::BAD_REQUEST.into_response();
    }
    let mut req_builder = axum::http::Request::builder().method("POST").uri(format!("/users/{username}/inbox"));
    for (name, value) in header.iter() {
        if let Ok(v) = value.to_str() {
            req_builder = req_builder.header(name, v);
        }
    }
    let request = match req_builder.body(body) {
        Ok(r) => r,
        Err(_) => return StatusCode::BAD_REQUEST.into_response(),
    };
    let (verified, bytes) = match verify_request(&state, request).await {
        VerifiedRequest::VerifiedDigest(req) => {
            let (bytes, digest_ok) = match req.into_body().collect_to_bytes().await {
                Ok(res) => res,
                Err(_) => return StatusCode::BAD_REQUEST.into_response(),
            };
            if !digest_ok {
                tracing::warn!("digest mismatch");
                return StatusCode::BAD_REQUEST.into_response();
            }
            (true, bytes)
        }
        VerifiedRequest::Verified(req) => {
            let Ok(collected) = http_body_util::BodyExt::collect(req.into_body()).await else {
                return StatusCode::BAD_REQUEST.into_response();
            };
            let bytes = collected.to_bytes();
            (true, bytes)
        }
        VerifiedRequest::CannotVerify(req) => {
            let Ok(collected) = http_body_util::BodyExt::collect(req.into_body()).await else {
                return StatusCode::BAD_REQUEST.into_response();
            };
            (false, collected.to_bytes())
        }
        VerifiedRequest::VerifyFailed => return StatusCode::BAD_REQUEST.into_response(),
    };
    let data = match String::from_utf8(bytes.to_vec()) {
        Ok(s) => s,
        Err(_) => return StatusCode::BAD_REQUEST.into_response(),
    };
    let queue_data = if let Ok(inbox) = serde_json::from_str::<InboxData>(&data) {
        tracing::info!("inbox data: {inbox:?}");
        QueueData::Inbox {
            username,
            ty: inbox.ty,
            id: inbox.id.clone(),
            verified_body: verified.then(|| data.clone()),
        }
    } else {
        tracing::error!("failed to parse inbox data: {data}");
        return StatusCode::BAD_REQUEST.into_response();
    };
    tracing::info!("enqueue data: {queue_data:?}");
    state.enqueue(queue_data).await;
    return StatusCode::ACCEPTED.into_response();

    #[derive(Debug, Deserialize)]
    struct InboxData {
        id: String,
        #[serde(rename = "type")]
        ty: String,
    }
}
