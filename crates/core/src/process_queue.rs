use crate::common::headers::{AP_ACCEPT, AP_RESPONSE_MIME};
use crate::common::macros::json_format;
use crate::common::{headers, sign};
use crate::traits::{ArticleNewComment, ArticleNewReaction, ArticleProvider, Env, HTTPClient, Queue, QueueData, UserProvider};
use axum::http::StatusCode;
use axum::http::header::{ACCEPT, CONTENT_TYPE};
use bytes::Bytes;
use chrono::{DateTime, FixedOffset};
use futures::TryStreamExt;
use http_body_util::{BodyExt, Limited};
use mime::Mime;
use serde::Deserialize;
use serde::de::DeserializeOwned;
use std::future;
use std::str::FromStr;
use url::Url;

#[derive(Debug, thiserror::Error)]
pub enum ProcessQueueError<RequestError> {
    #[error("{0}")]
    RequestError(RequestError),
}

#[derive(Debug)]
pub enum ProcessQueueResult {
    Finished,
    Retry,
}

#[tracing::instrument(skip(state))]
pub async fn process_queue<E>(state: &E, data: QueueData) -> ProcessQueueResult
where
    E: Env + ArticleProvider + UserProvider + HTTPClient + Queue + Send + Sync + Clone + 'static,
{
    tracing::info!("process queue: {:?}", data);
    match data {
        QueueData::Inbox {
            username,
            ty: _,
            id,
            verified_body,
        } => {
            let body_raw = if let Some(body) = verified_body {
                body
            } else {
                let Ok(b) = get_ap_data_raw(&id, state).await else {
                    return ProcessQueueResult::Finished;
                };
                String::from_utf8_lossy(&b).into_owned()
            };
            let body = match serde_json::from_str(&body_raw) {
                Ok(b) => b,
                Err(e) => {
                    tracing::warn!("failed to deserialize body: {e} raw={}", body_raw);
                    return ProcessQueueResult::Finished;
                }
            };
            tracing::info!("body: {:?}", body);
            match body {
                ResponseBody::Create {
                    object:
                        NoteObject {
                            id,
                            attributed_to,
                            published,
                            content,
                            reply_target,
                        },
                } => {
                    let reply_target = reply_target.into_string();
                    let slug = match reply_target.strip_prefix(&format!("{}/articles/", state.url())) {
                        Some(slug) => slug,
                        None => {
                            tracing::warn!("invalid reply target: {reply_target}");
                            return ProcessQueueResult::Finished;
                        }
                    };
                    if state.get_author_id(slug).await.is_none_or(|author| author != username) {
                        tracing::info!(slug, username, "author mismatch");
                        return ProcessQueueResult::Finished;
                    }
                    let comment = ArticleNewComment {
                        id,
                        author_id: attributed_to,
                        created_at: published.into(),
                        proceed_at: state.timestamp_now(),
                        content,
                        raw: body_raw,
                    };
                    tracing::info!("comment_data: {comment:#?}");
                    state.add_comment(slug, comment).await;
                    return ProcessQueueResult::Finished;
                }
                ResponseBody::Like { id, actor, object, content } => {
                    let slug = match object.strip_prefix(&format!("{}/articles/", state.url())) {
                        Some(slug) => slug,
                        None => {
                            tracing::warn!("invalid reaction target: {object}");
                            return ProcessQueueResult::Finished;
                        }
                    };
                    if state.get_author_id(slug).await.is_none_or(|author| author != username) {
                        tracing::info!(slug, username, "author mismatch");
                        return ProcessQueueResult::Finished;
                    }
                    let reaction = ArticleNewReaction {
                        id,
                        author_id: actor,
                        proceed_at: state.timestamp_now(),
                        reaction: content,
                        raw: body_raw,
                    };
                    tracing::info!("reaction_data: {reaction:#?}");
                    state.add_reaction(slug, reaction).await;
                    return ProcessQueueResult::Finished;
                }
                ResponseBody::Follow { id, actor, object } => {
                    let url = state.url();
                    if object != format!("{}/users/{username}", url) {
                        tracing::warn!(object, "invalid follow target");
                        return ProcessQueueResult::Finished;
                    }
                    #[derive(Debug, Deserialize)]
                    struct Person {
                        #[allow(dead_code)]
                        #[serde(rename = "id")]
                        _id: String,
                        #[serde(rename = "type")]
                        ty: String,
                        inbox: String,
                        #[serde(rename = "sharedInbox")]
                        shared_inbox: Option<String>,
                    }
                    let Ok(user): Result<Person, _> = get_ap_data(&actor, state).await else {
                        return ProcessQueueResult::Finished;
                    };
                    tracing::info!("body: {:?}", user);
                    if user.ty != "Person" {
                        tracing::warn!("invalid actor type");
                        return ProcessQueueResult::Finished;
                    }
                    state
                        .add_follower(&username, &actor, &user.shared_inbox.unwrap_or_else(|| user.inbox.clone()), &id)
                        .await;
                    let follow_actor = serde_json::to_string(&actor).unwrap();
                    let accept_actor = serde_json::to_string(&format!("{url}/users/{username}")).unwrap();
                    let object = serde_json::to_string(&id).unwrap();
                    let url = state.url();
                    let url = Url::parse_with_params(&format!("{url}/users/{username}/accept_follow"), [("object", &object)]).unwrap();
                    let url = serde_json::to_string(&url.to_string()).unwrap();
                    tracing::info!("url: {}", url);
                    let inbox = &user.inbox;
                    tracing::info!("inbox: {}", inbox);
                    let string = json_format! {
                        "@context": "https://www.w3.org/ns/activitystreams",
                        "id": url,
                        "type": "Accept",
                        "actor": accept_actor,
                        "object": {
                            "type": "Follow",
                            "actor": follow_actor,
                            "object": accept_actor,
                        },
                    };
                    tracing::info!("string: {}", string);
                    let Ok(request) = axum::http::Request::post(inbox)
                        .header(ACCEPT, AP_ACCEPT)
                        .header(CONTENT_TYPE, AP_RESPONSE_MIME)
                        .body(Bytes::from(string))
                    else {
                        tracing::warn!("failed to create post request");
                        return ProcessQueueResult::Finished;
                    };
                    let url = state.url();
                    let now = state.timestamp_now();
                    let key = state.signing_key();
                    let request = sign::sign(request, &format!("{url}/users/{username}#main-key"), key, now);
                    tracing::info!("request: {:?}", request);
                    let response = match state.request(request).await {
                        Ok(response) => response,
                        Err(e) => {
                            tracing::warn!("failed to fetch by: {:?}", e);
                            return ProcessQueueResult::Finished;
                        }
                    };
                    if !response.status().is_success() {
                        tracing::warn!("failed to post: {:?}", response);
                        let response = response
                            .into_body()
                            .into_data_stream()
                            .try_fold(Vec::new(), |mut acc, bytes| {
                                acc.extend_from_slice(&bytes);
                                future::ready(Ok(acc))
                            })
                            .await
                            .unwrap();
                        tracing::warn!("response: {:?}", String::from_utf8_lossy(&response));
                    }
                    return ProcessQueueResult::Finished;
                }
                ResponseBody::Undo { actor: undo_actor, object } => match *object {
                    ResponseBody::Like {
                        id: _,
                        actor,
                        object,
                        content: _,
                    } => {
                        if undo_actor != actor {
                            tracing::info!(undo_actor, actor, "actor mismatch");
                            return ProcessQueueResult::Finished;
                        }
                        let Some(slug) = object.strip_prefix(&format!("{}/articles/", state.url())) else {
                            tracing::warn!(object, "invalid reaction target");
                            return ProcessQueueResult::Finished;
                        };
                        if state.get_author_id(slug).await.is_none_or(|author| author != username) {
                            tracing::info!(slug, username, "author mismatch");
                            return ProcessQueueResult::Finished;
                        }
                        state.remove_reaction_by(slug, &actor).await;
                        return ProcessQueueResult::Finished;
                    }
                    ResponseBody::Follow { id: _, actor, object } => {
                        if undo_actor != actor {
                            tracing::info!(undo_actor, actor, "actor mismatch");
                            return ProcessQueueResult::Finished;
                        }
                        if object != format!("{}/users/{username}", state.url()) {
                            tracing::warn!(object, username, "invalid unfollow target");
                            return ProcessQueueResult::Finished;
                        }
                        state.remove_follower_by_actor(&username, &actor).await;
                        return ProcessQueueResult::Finished;
                    }
                    object => {
                        tracing::warn!(object = ?object, "invalid undo target");
                        return ProcessQueueResult::Finished;
                    }
                },
            }
        }
        QueueData::DeliveryNewArticleToAll { slug } => {
            let author = match state.get_author_id(&slug).await {
                Some(author) => author,
                None => {
                    tracing::warn!("author not found for slug: {}", slug);
                    return ProcessQueueResult::Finished;
                }
            };
            state
                .enqueue(QueueData::DeliveryNewArticleBatch {
                    slug: slug.clone(),
                    author,
                    last_inbox: String::new(),
                })
                .await;
            return ProcessQueueResult::Finished;
        }
        QueueData::DeliveryUpdateArticleToAll { slug } => {
            let author = match state.get_author_id(&slug).await {
                Some(author) => author,
                None => {
                    tracing::warn!("author not found for slug: {}", slug);
                    return ProcessQueueResult::Finished;
                }
            };
            state
                .enqueue(QueueData::DeliveryUpdateArticleBatch {
                    slug: slug.clone(),
                    author,
                    last_inbox: String::new(),
                })
                .await;
            return ProcessQueueResult::Finished;
        }
        QueueData::DeliveryDeleteArticleToAll { slug, author } => {
            state
                .enqueue(QueueData::DeliveryDeleteArticleBatch {
                    slug: slug.clone(),
                    author,
                    last_inbox: String::new(),
                })
                .await;
            return ProcessQueueResult::Finished;
        }
        QueueData::DeliveryNewArticleBatch { slug, author, last_inbox } => {
            let (inboxes, next_last) = state.get_followers_inbox_batch(&author, &last_inbox).await;
            let is_full = inboxes.is_full();
            for inbox in inboxes.into_iter() {
                state
                    .enqueue(QueueData::DeliveryNewArticle {
                        slug: slug.clone(),
                        author: author.clone(),
                        inbox,
                    })
                    .await;
            }
            if is_full {
                state
                    .enqueue(QueueData::DeliveryNewArticleBatch {
                        slug,
                        author,
                        last_inbox: next_last,
                    })
                    .await;
            }
            return ProcessQueueResult::Finished;
        }
        QueueData::DeliveryUpdateArticleBatch { slug, author, last_inbox } => {
            let (inboxes, next_last) = state.get_followers_inbox_batch(&author, &last_inbox).await;
            let is_full = inboxes.is_full();
            for inbox in inboxes.into_iter() {
                state
                    .enqueue(QueueData::DeliveryUpdateArticle {
                        slug: slug.clone(),
                        author: author.clone(),
                        inbox,
                    })
                    .await;
            }
            if is_full {
                state
                    .enqueue(QueueData::DeliveryUpdateArticleBatch {
                        slug,
                        author,
                        last_inbox: next_last,
                    })
                    .await;
            }
            return ProcessQueueResult::Finished;
        }
        QueueData::DeliveryDeleteArticleBatch { slug, author, last_inbox } => {
            let (inboxes, next_last) = state.get_followers_inbox_batch(&author, &last_inbox).await;
            let is_full = inboxes.is_full();
            for inbox in inboxes.into_iter() {
                state
                    .enqueue(QueueData::DeliveryDeleteArticle {
                        slug: slug.clone(),
                        author: author.clone(),
                        inbox,
                    })
                    .await;
            }
            if is_full {
                state
                    .enqueue(QueueData::DeliveryDeleteArticleBatch {
                        slug,
                        author,
                        last_inbox: next_last,
                    })
                    .await;
            }
            return ProcessQueueResult::Finished;
        }
        QueueData::DeliveryNewArticle { slug, author, inbox } => {
            let url = state.url();
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
            tracing::info!("body: {}", body);
            let request = axum::http::Request::post(inbox)
                .header(ACCEPT, AP_ACCEPT)
                .header(CONTENT_TYPE, AP_RESPONSE_MIME)
                .body(Bytes::from(body))
                .unwrap();
            let request = sign::sign(
                request,
                &format!("{url}/users/{author}#main-key"),
                state.signing_key(),
                state.timestamp_now(),
            );
            tracing::info!("request: {:?}", request);
            match state.request(request).await {
                Ok(response) => {
                    tracing::info!("response: {:?}", response);
                    if response.status().is_success() {
                        tracing::info!("posted");
                        return ProcessQueueResult::Finished;
                    } else {
                        tracing::warn!("failed to post");
                        let (parts, body) = response.into_parts();
                        let response_body = Limited::new(body, 1024 * 64)
                            .into_data_stream()
                            .try_fold(Vec::new(), |mut acc, bytes| {
                                acc.extend_from_slice(&bytes);
                                future::ready(Ok(acc))
                            })
                            .await;
                        tracing::warn!("response: {:?}", response_body.map(|body| String::from_utf8_lossy(&body).into_owned()));
                        return match parts.status {
                            StatusCode::GONE => ProcessQueueResult::Finished,
                            status if (400..500).contains(&status.as_u16()) => ProcessQueueResult::Finished,
                            _ => ProcessQueueResult::Retry,
                        };
                    }
                }
                Err(e) => {
                    tracing::error!("failed to post: {:?}", e);
                    return ProcessQueueResult::Finished;
                }
            }
        }
        QueueData::DeliveryUpdateArticle { slug, author, inbox } => {
            let url = state.url();
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
            tracing::info!("body: {}", body);
            let request = axum::http::Request::post(inbox)
                .header(ACCEPT, AP_ACCEPT)
                .header(CONTENT_TYPE, AP_RESPONSE_MIME)
                .body(Bytes::from(body))
                .unwrap();
            let request = sign::sign(
                request,
                &format!("{url}/users/{author}#main-key"),
                state.signing_key(),
                state.timestamp_now(),
            );
            tracing::info!("request: {:?}", request);
            match state.request(request).await {
                Ok(response) => {
                    tracing::info!("response: {:?}", response);
                    if response.status().is_success() {
                        tracing::info!("posted");
                        return ProcessQueueResult::Finished;
                    } else {
                        tracing::warn!("failed to post");
                        let (parts, body) = response.into_parts();
                        let response_body = Limited::new(body, 1024 * 64)
                            .into_data_stream()
                            .try_fold(Vec::new(), |mut acc, bytes| {
                                acc.extend_from_slice(&bytes);
                                future::ready(Ok(acc))
                            })
                            .await;
                        tracing::warn!("response: {:?}", response_body.map(|body| String::from_utf8_lossy(&body).into_owned()));
                        return match parts.status {
                            StatusCode::GONE => ProcessQueueResult::Finished,
                            status if (400..500).contains(&status.as_u16()) => ProcessQueueResult::Finished,
                            _ => ProcessQueueResult::Retry,
                        };
                    }
                }
                Err(e) => {
                    tracing::error!("failed to post: {:?}", e);
                    return ProcessQueueResult::Finished;
                }
            }
        }
        QueueData::DeliveryDeleteArticle { slug, author, inbox } => {
            let url = state.url();
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
            tracing::info!("body: {}", body);
            let request = axum::http::Request::post(inbox)
                .header(ACCEPT, AP_ACCEPT)
                .header(CONTENT_TYPE, AP_RESPONSE_MIME)
                .body(Bytes::from(body))
                .unwrap();
            let request = sign::sign(
                request,
                &format!("{url}/users/{author}#main-key"),
                state.signing_key(),
                state.timestamp_now(),
            );
            tracing::info!("request: {:?}", request);
            match state.request(request).await {
                Ok(response) => {
                    tracing::info!("response: {:?}", response);
                    if response.status().is_success() {
                        tracing::info!("posted");
                        return ProcessQueueResult::Finished;
                    } else {
                        tracing::warn!("failed to post");
                        let (parts, body) = response.into_parts();
                        let response_body = Limited::new(body, 1024 * 64)
                            .into_data_stream()
                            .try_fold(Vec::new(), |mut acc, bytes| {
                                acc.extend_from_slice(&bytes);
                                future::ready(Ok(acc))
                            })
                            .await;
                        tracing::warn!("response: {:?}", response_body.map(|body| String::from_utf8_lossy(&body).into_owned()));
                        return match parts.status {
                            StatusCode::GONE => ProcessQueueResult::Finished,
                            status if (400..500).contains(&status.as_u16()) => ProcessQueueResult::Finished,
                            _ => ProcessQueueResult::Retry,
                        };
                    }
                }
                Err(e) => {
                    tracing::error!("failed to post: {:?}", e);
                    return ProcessQueueResult::Finished;
                }
            }
        }
    }

    #[derive(Debug, Deserialize)]
    #[serde(tag = "type")]
    enum ResponseBody {
        Create {
            object: NoteObject,
        },
        Like {
            id: String,
            actor: String,
            object: String,
            #[serde(default)]
            content: String,
        },
        Follow {
            id: String,
            actor: String,
            object: String,
        },
        Undo {
            actor: String,
            object: Box<ResponseBody>,
        },
    }
    #[derive(Debug, Deserialize)]
    struct NoteObject {
        id: String,
        #[serde(rename = "attributedTo")]
        attributed_to: String,
        published: DateTime<FixedOffset>,
        content: String,
        #[serde(flatten)]
        reply_target: ReplyTarget,
    }
    #[derive(Debug, Deserialize)]
    enum ReplyTarget {
        #[serde(rename = "inReplyTo")]
        InReplyTo(String),
        #[serde(rename = "quoteUri")]
        QuoteUri(String),
        #[serde(rename = "quoteUrl")]
        QuoteUrl(String),
    }
    impl ReplyTarget {
        fn into_string(self) -> String {
            match self {
                ReplyTarget::InReplyTo(id) => id,
                ReplyTarget::QuoteUri(uri) => uri,
                ReplyTarget::QuoteUrl(url) => url,
            }
        }
    }
}

#[tracing::instrument(skip(state))]
async fn get_ap_data_raw<E>(id: &str, state: &E) -> Result<Vec<u8>, ()>
where
    E: HTTPClient,
{
    let Ok(request) = axum::http::Request::get(id).header(ACCEPT, AP_ACCEPT).body(Bytes::new()) else {
        tracing::warn!("failed to create get request");
        return Err(());
    };
    let response = match state.request(request).await {
        Ok(response) => response,
        Err(e) => {
            tracing::warn!("failed to fetch by: {:?}", e);
            return Err(());
        }
    };
    if !response.status().is_success() {
        tracing::warn!("failed to fetch: {:?}", response);
        return Err(());
    }
    if !response
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|ty| ty.to_str().ok())
        .and_then(|ty| Mime::from_str(ty).ok())
        .is_some_and(|ty| headers::is_content_type_ap(&ty))
    {
        tracing::warn!("invalid response from server: {:?}", response);
        return Err(());
    }
    let body = response.into_body();
    let body = Limited::new(body, 1024 * 64);
    match body
        .into_data_stream()
        .try_fold(Vec::new(), |mut acc, bytes| {
            acc.extend_from_slice(&bytes);
            future::ready(Ok(acc))
        })
        .await
    {
        Ok(body) => Ok(body),
        Err(e) => {
            tracing::warn!("failed to collect response: {:?}", e);
            Err(())
        }
    }
}

#[tracing::instrument(skip(state))]
async fn get_ap_data<E, R>(id: &str, state: &E) -> Result<R, ()>
where
    E: HTTPClient,
    R: DeserializeOwned,
{
    let body = get_ap_data_raw(id, state).await?;
    match serde_json::from_slice::<R>(&body) {
        Ok(body) => Ok(body),
        Err(_) => {
            tracing::warn!("failed to parse response: {}", String::from_utf8_lossy(&body));
            Err(())
        }
    }
}
