use crate::traits::{HTTPClient, Queue, QueueData, UserProvider};
use crate::verify::{VerifyResult, verify_request};
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
    E: UserProvider + Queue + HTTPClient,
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
    let verified = match verify_request(&state, &header, "POST", &format!("/users/{username}/inbox"), data.as_bytes()).await {
        VerifyResult::Verified => true,
        VerifyResult::CannotVerify => false,
        VerifyResult::Failed => {
            return StatusCode::BAD_REQUEST.into_response();
        }
    };
    let queue_data = if let Ok(inbox_data) = serde_json::from_str::<SpecializedInboxData>(&data) {
        tracing::info!("specialized inbox data: {inbox_data:?}");
        match inbox_data {
            SpecializedInboxData::Follow { actor, object, id } => QueueData::Follow {
                username,
                actor,
                object,
                id,
                verified,
            },
            SpecializedInboxData::Undo { object } => match object {
                UndoObject::Follow { id, actor, object } => QueueData::Unfollow {
                    username,
                    id: id.into_id(),
                    actor,
                    object,
                },
            },
        }
    } else if let Ok(inbox) = serde_json::from_str::<InboxData>(&data) {
        tracing::info!("inbox data: {inbox:?}");
        QueueData::Inbox {
            username,
            ty: inbox.ty,
            id: inbox.id.clone(),
            body: if verified { Some(data.clone()) } else { None },
            verified,
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
        Follow {
            id: AnyId,
            #[serde(default)]
            actor: Option<String>,
            #[serde(default)]
            object: Option<String>,
        },
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
