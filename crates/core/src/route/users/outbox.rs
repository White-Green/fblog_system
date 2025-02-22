use crate::traits::UserProvider;
use axum::body::Body;
use axum::extract::{Path, State};
use axum::response::{IntoResponse, Response};

pub async fn user_outbox_get<E>(Path(_username): Path<String>, State(_state): State<E>) -> Response<Body>
where
    E: UserProvider,
{
    ().into_response()
}
