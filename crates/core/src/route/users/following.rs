use crate::common::headers::{AP_RESPONSE_MIME, AcceptMimeSet, HeaderReader};
use crate::common::macros::json_format;
use crate::traits::{Env, UserProvider};
use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::header::CONTENT_TYPE;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};

#[tracing::instrument(skip(state))]
pub async fn user_following_get<E>(header: HeaderMap, Path(username): Path<String>, State(state): State<E>) -> Response<Body>
where
    E: Env + UserProvider,
{
    let header = HeaderReader::new(&header);
    if header.select(AcceptMimeSet::AP).is_none() {
        tracing::info!("not accepted ap");
        return StatusCode::NOT_ACCEPTABLE.into_response();
    }
    if !state.exists_user(&username).await {
        tracing::info!("user is not found");
        return StatusCode::NOT_FOUND.into_response();
    }
    let url = state.url();
    let id = serde_json::to_string(&format!("{url}/users/{username}/following")).unwrap();
    let body = json_format! {
        "@context": "https://www.w3.org/ns/activitystreams",
        "id": id,
        "type": "OrderedCollection",
        "totalItems": 0,
        "items": [],
    };
    tracing::info!("body: {}", body);
    Response::builder().header(CONTENT_TYPE, AP_RESPONSE_MIME).body(Body::from(body)).unwrap()
}
