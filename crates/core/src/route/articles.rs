use crate::common::headers::{AP_RESPONSE_MIME, AcceptMime, AcceptMimeSet, HeaderReader};
use crate::traits::{ArticleProvider, Env};
use axum::Json;
use axum::body::Body;
use axum::extract::{Path, Query, State};
use axum::http::header::CONTENT_TYPE;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};

pub(crate) mod events;

#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
enum ArticleData {
    Meta,
}

#[derive(Debug, Deserialize)]
pub struct ArticleDataQuery {
    data: Option<ArticleData>,
}

#[derive(Debug, Serialize)]
struct ArticleMetadata {
    comment_count: usize,
    reaction_count: usize,
}

#[tracing::instrument(skip(state))]
pub async fn article_get<E>(header: HeaderMap, slug: String, state: E) -> Response<Body>
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

#[tracing::instrument(skip(state))]
async fn article_metadata_get<E>(header: HeaderMap, slug: String, state: E) -> Response<Body>
where
    E: ArticleProvider,
{
    let header = HeaderReader::new(&header);
    match header.select(AcceptMimeSet::JSON) {
        Some(AcceptMime::Json) => {
            tracing::info!("accept json");
            let (comment_count, reaction_count) = futures::join!(state.comment_count(&slug), state.reaction_count(&slug),);
            let metadata = ArticleMetadata {
                comment_count,
                reaction_count,
            };
            tracing::info!("{metadata:?}");
            Json(metadata).into_response()
        }
        _ => {
            tracing::info!("not accepted");
            StatusCode::NOT_ACCEPTABLE.into_response()
        }
    }
}

#[tracing::instrument(skip(state))]
pub async fn article_or_comments_get<E>(
    header: HeaderMap,
    Path(slug): Path<String>,
    Query(query): Query<ArticleDataQuery>,
    State(state): State<E>,
) -> Response<Body>
where
    E: Env + ArticleProvider + Clone,
{
    match query.data {
        None => article_get(header, slug, state).await,
        Some(ArticleData::Meta) => article_metadata_get(header, slug, state).await,
    }
}
