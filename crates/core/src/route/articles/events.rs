use crate::common::headers::{AP_RESPONSE_MIME, AcceptMime, AcceptMimeSet, HeaderReader};
use crate::json_format;
use crate::traits::{ArticleProvider, Env};
use axum::body::Body;
use axum::extract::{Path, Query, State};
use axum::http::header::CONTENT_TYPE;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use serde::Deserialize;

#[tracing::instrument(skip(state))]
pub async fn article_create_events_get<E>(header: HeaderMap, Path(slug): Path<String>, State(state): State<E>) -> Response<Body>
where
    E: Env + ArticleProvider,
{
    let header = HeaderReader::new(&header);
    let Some(AcceptMime::AP) = header.select(AcceptMimeSet::AP) else {
        return StatusCode::NOT_ACCEPTABLE.into_response();
    };
    if !state.exists_article(&slug).await {
        return StatusCode::NOT_FOUND.into_response();
    }

    let url = state.url();
    let author = state.get_author_id(&slug).await.unwrap();
    let actor = serde_json::to_string(&format!("{url}/users/{author}")).unwrap();
    let id = serde_json::to_string(&format!("{url}/events/articles/create/{slug}")).unwrap();
    let object = serde_json::to_string(&format!("{url}/articles/{slug}")).unwrap();
    let body = json_format! {
        "@context": "https://www.w3.org/ns/activitystreams",
        "id": id,
        "type": "Create",
        "actor": actor,
        "object": object,
    };
    Response::builder()
        .header(CONTENT_TYPE, AP_RESPONSE_MIME)
        .body(body)
        .unwrap()
        .into_response()
}

#[tracing::instrument(skip(state))]
pub async fn article_update_events_get<E>(header: HeaderMap, Path(slug): Path<String>, State(state): State<E>) -> Response<Body>
where
    E: Env + ArticleProvider,
{
    let header = HeaderReader::new(&header);
    let Some(AcceptMime::AP) = header.select(AcceptMimeSet::AP) else {
        return StatusCode::NOT_ACCEPTABLE.into_response();
    };
    if !state.exists_article(&slug).await {
        return StatusCode::NOT_FOUND.into_response();
    }

    let url = state.url();
    let author = state.get_author_id(&slug).await.unwrap();
    let actor = serde_json::to_string(&format!("{url}/users/{author}")).unwrap();
    let id = serde_json::to_string(&format!("{url}/events/articles/update/{slug}")).unwrap();
    let object = serde_json::to_string(&format!("{url}/articles/{slug}")).unwrap();
    let body = json_format! {
        "@context": "https://www.w3.org/ns/activitystreams",
        "id": id,
        "type": "Update",
        "actor": actor,
        "object": object,
    };
    Response::builder()
        .header(CONTENT_TYPE, AP_RESPONSE_MIME)
        .body(body)
        .unwrap()
        .into_response()
}

#[tracing::instrument(skip(state))]
pub async fn article_delete_events_get<E>(header: HeaderMap, Path(slug): Path<String>, State(state): State<E>) -> Response<Body>
where
    E: Env + ArticleProvider,
{
    let header = HeaderReader::new(&header);
    let Some(AcceptMime::AP) = header.select(AcceptMimeSet::AP) else {
        return StatusCode::NOT_ACCEPTABLE.into_response();
    };
    if state.exists_article(&slug).await {
        return StatusCode::NOT_FOUND.into_response();
    }

    let url = state.url();
    let author = state.get_author_id(&slug).await.unwrap();
    let actor = serde_json::to_string(&format!("{url}/users/{author}")).unwrap();
    let id = serde_json::to_string(&format!("{url}/events/articles/delete/{slug}")).unwrap();
    let object = serde_json::to_string(&format!("{url}/articles/{slug}")).unwrap();
    let body = json_format! {
        "@context": "https://www.w3.org/ns/activitystreams",
        "id": id,
        "type": "Delete",
        "actor": actor,
        "object": object,
    };
    Response::builder()
        .header(CONTENT_TYPE, AP_RESPONSE_MIME)
        .body(body)
        .unwrap()
        .into_response()
}
