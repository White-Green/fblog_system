use crate::common::headers::{AcceptMimeSet, HeaderReader};
use crate::traits::{ArticleComment, ArticleProvider};
use arrayvec::ArrayVec;
use axum::Json;
use axum::body::Body;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
pub struct ArticleCommentQuery {
    until: Option<u64>,
}

#[tracing::instrument(skip(state))]
pub async fn article_comments_get<E>(
    header: HeaderMap,
    Path(slug): Path<String>,
    Query(query): Query<ArticleCommentQuery>,
    State(state): State<E>,
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
        let (comments, next_until) = state.get_public_comments_until(&slug, query.until.unwrap_or(u64::MAX)).await;
        let result = ArticleCommentsResult {
            comments,
            next: format!("/articles/{slug}/comments?until={next_until}"),
        };
        Json(result).into_response()
    } else {
        StatusCode::NOT_FOUND.into_response()
    }
}
