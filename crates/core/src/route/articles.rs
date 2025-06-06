use crate::common::headers::{AP_RESPONSE_MIME, AcceptMime, AcceptMimeSet, HeaderReader};
use crate::traits::{ArticleComment, ArticleProvider, Env};
use arrayvec::ArrayVec;
use axum::body::Body;
use axum::extract::{Path, Query, State};
use axum::http::header::CONTENT_TYPE;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::{Deserialize, Serialize};

pub(crate) mod events;

#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
enum ArticleData {
    Comments,
}

#[derive(Debug, Deserialize)]
pub struct ArticleDataQuery {
    data: Option<ArticleData>,
    until: Option<u64>,
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
                    Response::builder()
                        .header(CONTENT_TYPE, mime::TEXT_HTML.as_ref())
                        .body(body)
                        .unwrap()
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
                    Response::builder()
                        .header(CONTENT_TYPE, AP_RESPONSE_MIME)
                        .body(body)
                        .unwrap()
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
pub async fn article_or_comments_get<E>(
    header: HeaderMap,
    Path(slug): Path<String>,
    Query(query): Query<ArticleDataQuery>,
    State(state): State<E>,
) -> Response<Body>
where
    E: Env + ArticleProvider + Clone,
{
    if matches!(query.data, Some(ArticleData::Comments)) {
        return article_comments_get(header, slug, query.until, state).await;
    }
    article_get(header, slug, state).await
}

#[tracing::instrument(skip(state))]
async fn article_comments_get<E>(
    header: HeaderMap,
    slug: String,
    until: Option<u64>,
    state: E,
) -> Response<Body>
where
    E: ArticleProvider,
{
    let header = HeaderReader::new(&header);
    if header.select(AcceptMimeSet::JSON).is_none() {
        tracing::info!("not accepted");
        return StatusCode::NOT_ACCEPTABLE.into_response();
    }
    if state.exists_article(&slug).await {
        #[derive(Debug, Serialize)]
        struct ArticleCommentsResult {
            comments: ArrayVec<ArticleComment, 10>,
            next: String,
        }
        let (comments, next_until) =
            state.get_public_comments_until(&slug, until.unwrap_or(u64::MAX)).await;
        let result = ArticleCommentsResult {
            comments,
            next: format!("/articles/{slug}?data=comments&until={next_until}"),
        };
        Json(result).into_response()
    } else {
        StatusCode::NOT_FOUND.into_response()
    }
}
