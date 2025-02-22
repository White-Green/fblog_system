use crate::common::headers::{AP_RESPONSE_MIME, AcceptMimeSet, HeaderReader};
use crate::common::macros::json_format;
use crate::traits::{Env, UserProvider};
use axum::body::Body;
use axum::extract::{Path, Query, State};
use axum::http::header::CONTENT_TYPE;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use url::Url;

#[derive(Debug, Deserialize)]
pub struct AcceptFollowQuery {
    object: String,
}
#[tracing::instrument(skip(state))]
pub async fn user_accept_follow_get<E>(
    header: HeaderMap,
    Path(username): Path<String>,
    Query(query): Query<AcceptFollowQuery>,
    State(state): State<E>,
) -> Response<Body>
where
    E: Env + UserProvider,
{
    if !state.exists_user(&username).await {
        tracing::info!("user is not found");
        return StatusCode::NOT_FOUND.into_response();
    }
    let header = HeaderReader::new(&header);
    if header.select(AcceptMimeSet::AP).is_none() {
        tracing::info!("not accepted ap");
        return StatusCode::NOT_ACCEPTABLE.into_response();
    }
    let url = state.url();
    let url = Url::parse_with_params(&format!("{url}/users/{username}/accept_follow"), [("object", &query.object)]).unwrap();
    let accept_actor = serde_json::to_string(&format!("{url}/users/{username}")).unwrap();
    let follow_actor = serde_json::to_string(&query.object).unwrap();
    let body = json_format! {
        "@context": "https://www.w3.org/ns/activitystreams",
        "id": url,
        "type": "Accept",
        "actor": accept_actor,
        "object": {
            "type": "Follow",
            "actor": follow_actor,
            "object": accept_actor,
        },
    };
    tracing::info!("body: {}", body);
    Response::builder()
        .header(CONTENT_TYPE, AP_RESPONSE_MIME)
        .body(body)
        .unwrap()
        .into_response()
}
