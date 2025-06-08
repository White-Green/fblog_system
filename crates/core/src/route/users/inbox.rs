use crate::traits::{Queue, QueueData, UserProvider};
use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::header::CONTENT_TYPE;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use mime::Mime;
use serde::Deserialize;
use std::str::FromStr;

#[tracing::instrument(skip(state))]
pub async fn user_inbox_post<E>(header: HeaderMap, Path(username): Path<String>, State(state): State<E>, data: String) -> Response<Body>
where
    E: UserProvider + Queue,
{
    if !state.exists_user(&username).await {
        tracing::info!("user is not found");
        return StatusCode::NOT_FOUND.into_response();
    }
    if !header
        .get(CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| Mime::from_str(v).ok())
        .is_some_and(|ty| crate::common::headers::is_content_type_ap(&ty))
    {
        tracing::info!("invalid content type");
        return StatusCode::BAD_REQUEST.into_response();
    }
    let queue_data = if let Ok(data) = serde_json::from_str::<SpecializedInboxData>(&data) {
        tracing::info!("specialized inbox data: {data:?}");
        match data {
            SpecializedInboxData::Follow { actor, object, id } => QueueData::Follow { username, actor, object, id },
            SpecializedInboxData::Undo { object } => match object {
                UndoObject::Follow { id } => QueueData::Unfollow { username, id: id.into_id() },
            },
        }
    } else if let Ok(data) = serde_json::from_str::<InboxData>(&data) {
        tracing::info!("inbox data: {data:?}");
        QueueData::Inbox {
            username,
            ty: data.ty,
            id: data.id,
        }
    } else {
        tracing::error!("failed to parse inbox data: {data}");
        return StatusCode::BAD_REQUEST.into_response();
    };
    tracing::info!("enqueue data: {queue_data:?}");
    state.enqueue(queue_data).await;
    return StatusCode::ACCEPTED.into_response();

    #[derive(Debug, Deserialize)]
    #[serde(tag = "type")]
    enum SpecializedInboxData {
        Follow { actor: String, object: String, id: String },
        Undo { object: UndoObject },
    }

    #[derive(Debug, Deserialize)]
    #[serde(tag = "type")]
    enum UndoObject {
        Follow { id: AnyId },
    }

    #[derive(Debug, Deserialize)]
    #[serde(untagged)]
    enum AnyId {
        String(String),
        Object { id: String },
    }

    impl AnyId {
        fn into_id(self) -> String {
            match self {
                AnyId::String(id) => id,
                AnyId::Object { id } => id,
            }
        }
    }
    #[derive(Debug, Deserialize)]
    struct InboxData {
        id: String,
        #[serde(rename = "type")]
        ty: String,
    }
}
