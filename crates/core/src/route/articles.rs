use crate::common::headers::{AP_RESPONSE_MIME, AcceptMime, AcceptMimeSet, HeaderReader};
use crate::json_format;
use crate::traits::{ArticleProvider, Env};
use axum::body::Body;
use axum::extract::{Path, Query, State};
use axum::http::header::CONTENT_TYPE;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use serde::Deserialize;

pub(crate) mod comments;
pub(crate) mod events;

#[tracing::instrument(skip(state))]
pub async fn article_get<E>(
    header: HeaderMap,
    Path(slug): Path<String>,
    State(state): State<E>,
) -> Response<Body>
where
    E: Env + ArticleProvider,
{
    let header = HeaderReader::new(&header);
    match header.select(AcceptMimeSet::HTML | AcceptMimeSet::AP) {
        Some(AcceptMime::Html) => {
            tracing::info!("accept html");
            match state.get_article_html(&slug).await {
                Some(body) => {
                    tracing::info!("found article");
                    Response::builder().header(CONTENT_TYPE, mime::TEXT_HTML.as_ref()).body(body).unwrap()
                }
                None => {
                    tracing::info!("article is not found");
                    StatusCode::NOT_FOUND.into_response()
                }
            }
        }
        Some(AcceptMime::AP) => {
            tracing::info!("accept ap");
            match state.get_article_ap(&slug).await {
                Some(body) => {
                    tracing::info!("found article");
                    Response::builder().header(CONTENT_TYPE, AP_RESPONSE_MIME).body(body).unwrap()
                }
                None => {
                    tracing::info!("article is not found");
                    StatusCode::NOT_FOUND.into_response()
                }
            }
        }
        _ => {
            tracing::info!("not accepted");
            StatusCode::NOT_ACCEPTABLE.into_response()
        }
    }
}
