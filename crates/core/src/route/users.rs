use crate::common::headers::{AP_RESPONSE_MIME, AcceptMime, AcceptMimeSet, HeaderReader};
use crate::traits::UserProvider;
use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::header::CONTENT_TYPE;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};

pub(crate) mod accept_follow;
pub(crate) mod followers;
pub(crate) mod following;
pub(crate) mod inbox;
pub(crate) mod outbox;

#[tracing::instrument(skip(state))]
pub async fn user_get<E>(header: HeaderMap, Path(username): Path<String>, State(state): State<E>) -> Response<Body>
where
    E: UserProvider,
{
    let header = HeaderReader::new(&header);
    match header.select(AcceptMimeSet::HTML | AcceptMimeSet::AP) {
        Some(AcceptMime::Html) => {
            tracing::info!("accept html");
            match state.get_user_html(&username).await {
                Some(body) => {
                    tracing::info!("found user");
                    Response::builder().header(CONTENT_TYPE, mime::TEXT_HTML.as_ref()).body(body).unwrap()
                }
                None => {
                    tracing::info!("user is not found");
                    StatusCode::NOT_FOUND.into_response()
                }
            }
        }
        Some(AcceptMime::AP) => {
            tracing::info!("accept ap");
            match state.get_user_ap(&username).await {
                Some(body) => {
                    tracing::info!("found user");
                    Response::builder().header(CONTENT_TYPE, AP_RESPONSE_MIME).body(body).unwrap()
                }
                None => {
                    tracing::info!("user is not found");
                    StatusCode::NOT_FOUND.into_response()
                }
            }
        }
        _ => {
            tracing::info!("not accepted");
            Response::builder().status(StatusCode::NOT_ACCEPTABLE).body(Body::empty()).unwrap()
        }
    }
}
