use crate::common::headers::{AP_RESPONSE_MIME, AcceptMimeSet, HeaderReader};
use crate::common::macros::json_format;
use crate::traits::{Env, UserProvider};
use axum::body::Body;
use axum::extract::{Query, State};
use axum::http::header::CONTENT_TYPE;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use regex::Regex;
use serde::Deserialize;
use std::sync::LazyLock;

#[derive(Debug, Deserialize)]
pub struct WebFingerQuery {
    resource: String,
}

#[tracing::instrument(skip(state))]
pub async fn get_webfinger<E>(header: HeaderMap, Query(query): Query<WebFingerQuery>, State(state): State<E>) -> Response<Body>
where
    E: Env + UserProvider,
{
    let header = HeaderReader::new(&header);
    if header.select(AcceptMimeSet::JSON).is_none() {
        tracing::info!("not accepted");
        return StatusCode::NOT_ACCEPTABLE.into_response();
    }
    static REGEX: LazyLock<Regex> = LazyLock::new(|| Regex::new("\\Aacct:(.+)@(.+)\\z").unwrap());
    let Some(capture) = REGEX.captures(&query.resource) else {
        tracing::info!("invalid resource");
        return StatusCode::NOT_FOUND.into_response();
    };
    let username = capture.get(1).unwrap();
    let username = username.as_str();
    if !state.exists_user(username).await {
        tracing::info!("user is not found");
        return StatusCode::NOT_FOUND.into_response();
    }
    let query_host = capture.get(2).unwrap();
    let url = state.url();
    let url_string = url.to_string();
    let hostname = url_string
        .strip_prefix("https://")
        .or_else(|| url_string.strip_prefix("http://"))
        .unwrap();
    let hostname = hostname.strip_suffix("/").unwrap_or(hostname);
    if query_host.as_str() != hostname {
        tracing::info!("invalid host");
        return StatusCode::NOT_FOUND.into_response();
    }
    let subject = serde_json::to_string(&query.resource).unwrap();
    let ty = serde_json::to_string(AP_RESPONSE_MIME).unwrap();
    let href = serde_json::to_string(&format_args!("{}/users/{}", url, username)).unwrap();
    let body = json_format! {
        "subject": subject,
        "links": [
            {
                "rel":  "self",
                "type": ty,
                "href": href,
            },
        ],
    };
    tracing::info!("body: {}", body);
    axum::http::Response::builder()
        .header(CONTENT_TYPE, mime::APPLICATION_JSON.as_ref())
        .body(Body::from(body))
        .unwrap()
}
