use crate::common::headers::{AP_RESPONSE_MIME, AcceptMime, AcceptMimeSet, HeaderReader};
use crate::common::macros::json_format;
use crate::traits::{Env, UserProvider};
use axum::body::Body;
use axum::extract::{Path, Query, State};
use axum::http::header::{CONTENT_TYPE, LOCATION};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct UserFollowingQuery {
    until: Option<u64>,
}

#[tracing::instrument(skip(state))]
pub async fn user_followers_get<E>(
    header: HeaderMap,
    Path(username): Path<String>,
    Query(query): Query<UserFollowingQuery>,
    State(state): State<E>,
) -> Response<Body>
where
    E: Env + UserProvider,
{
    if !state.exists_user(&username).await {
        return StatusCode::NOT_FOUND.into_response();
    }
    let url = state.url();
    let header = HeaderReader::new(&header);
    match header.select(AcceptMimeSet::HTML | AcceptMimeSet::AP | AcceptMimeSet::JSON) {
        Some(AcceptMime::Html) => {
            tracing::info!("accept html");
            if query.until.is_some() {
                tracing::info!("redirect to /users/{username}/followers");
                Response::builder()
                    .status(StatusCode::PERMANENT_REDIRECT)
                    .header(LOCATION, format!("/users/{username}/followers"))
                    .body(Body::empty())
                    .unwrap()
            } else {
                tracing::info!("get followers html");
                let body = state.get_followers_html(&username).await.unwrap();
                Response::builder().header(CONTENT_TYPE, mime::TEXT_HTML.as_ref()).body(body).unwrap()
            }
        }
        Some(AcceptMime::AP) => {
            tracing::info!("accept ap");
            if let Some(until_timestamp) = query.until {
                tracing::info!("get followers ap as OrderedCollectionPage");
                let (followers, next_timestamp) = state.get_follower_ids_until(&username, until_timestamp).await;
                let id = serde_json::to_string(&format!("{url}/users/{username}/followers?until={until_timestamp}")).unwrap();
                let part_of = serde_json::to_string(&format!("{url}/users/{username}/followers")).unwrap();
                let items = serde_json::to_string(&*followers).unwrap();
                let body = if followers.is_full() {
                    let next = serde_json::to_string(&format!("{url}/users/{username}/followers?until={next_timestamp}")).unwrap();
                    json_format! {
                        "@context": "https://www.w3.org/ns/activitystreams",
                        "type": "OrderedCollectionPage",
                        "id": id,
                        "partOf": part_of,
                        "next": next,
                        "items": items,
                    }
                } else {
                    json_format! {
                        "@context": "https://www.w3.org/ns/activitystreams",
                        "type": "OrderedCollectionPage",
                        "id": id,
                        "partOf": part_of,
                        "items": items,
                    }
                };
                tracing::info!("body: {}", body);
                Response::builder().header(CONTENT_TYPE, AP_RESPONSE_MIME).body(Body::from(body)).unwrap()
            } else {
                tracing::info!("get followers ap as OrderedCollection");
                let followers_len = state.get_followers_len(&username).await;
                let id = serde_json::to_string(&format!("{url}/users/{username}/followers")).unwrap();
                let first = serde_json::to_string(&format!("{url}/users/{username}/followers?until={}", i64::MAX)).unwrap();
                let body = json_format! {
                    "@context": "https://www.w3.org/ns/activitystreams",
                    "type": "OrderedCollection",
                    "id": id,
                    "totalItems": followers_len,
                    "first": first,
                };
                tracing::info!("body: {}", body);
                Response::builder().header(CONTENT_TYPE, AP_RESPONSE_MIME).body(Body::from(body)).unwrap()
            }
        }
        Some(AcceptMime::Json) => {
            tracing::info!("accept json");
            if query.until.is_some() {
                todo!()
            } else {
                Response::builder()
                    .status(StatusCode::PERMANENT_REDIRECT)
                    .header(LOCATION, format!("/users/{username}/followers?until={}", i64::MAX))
                    .body(Body::empty())
                    .unwrap()
            }
        }
        _ => {
            tracing::info!("not accepted");
            StatusCode::NOT_ACCEPTABLE.into_response()
        }
    }
}
