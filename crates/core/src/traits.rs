use arrayvec::ArrayVec;
use axum::body::Body;
use axum::http::Request;
use bytes::Bytes;
use chrono::{DateTime, Utc};
use rsa::pkcs1v15::SigningKey;
use serde::{Deserialize, Serialize};
use std::error::Error;
use std::fmt::Display;

pub type RSASHA2SigningKey = SigningKey<rsa::sha2::Sha256>;

pub trait Env {
    fn url(&self) -> impl Display + Send + '_;
    fn timestamp_now(&self) -> DateTime<Utc>;
    fn signing_key(&self) -> &RSASHA2SigningKey;
}

#[derive(Debug, Serialize)]
pub struct ArticleNewComment {
    pub id: String,
    pub author_id: String,
    pub created_at: DateTime<Utc>,
    pub proceed_at: DateTime<Utc>,
    pub content: String,
    pub raw: String,
}

#[derive(Debug, Serialize)]
pub struct ArticleNewReaction {
    pub id: String,
    pub author_id: String,
    pub reaction: String,
    pub proceed_at: DateTime<Utc>,
    pub raw: String,
}

pub trait ArticleProvider {
    fn exists_article(&self, slug: &str) -> impl Future<Output = bool> + Send;
    fn get_article_html(&self, slug: &str) -> impl Future<Output = Option<Body>> + Send;
    fn get_article_ap(&self, slug: &str) -> impl Future<Output = Option<Body>> + Send;
    fn get_author_id(&self, slug: &str) -> impl Future<Output = Option<String>> + Send;

    fn add_comment(&self, slug: &str, comment: ArticleNewComment) -> impl Future<Output = ()> + Send;
    fn add_reaction(&self, slug: &str, reaction: ArticleNewReaction) -> impl Future<Output = ()> + Send;
    fn remove_reaction_by(&self, slug: &str, actor: &str) -> impl Future<Output = ()> + Send;
    fn comment_count(&self, slug: &str) -> impl Future<Output = usize> + Send;
    fn reaction_count(&self, slug: &str) -> impl Future<Output = usize> + Send;
}

pub trait UserProvider {
    fn exists_user(&self, username: &str) -> impl Future<Output = bool> + Send;
    fn get_user_html(&self, username: &str) -> impl Future<Output = Option<Body>> + Send;
    fn get_user_ap(&self, username: &str) -> impl Future<Output = Option<Body>> + Send;

    fn get_followers_html(&self, username: &str) -> impl Future<Output = Option<Body>> + Send;
    fn get_followers_len(&self, username: &str) -> impl Future<Output = usize> + Send;
    fn get_follower_ids_until(&self, username: &str, until: u64) -> impl Future<Output = (ArrayVec<String, 10>, u64)> + Send;

    fn add_follower(&self, username: &str, follower_id: &str, inbox: &str, event_id: &str) -> impl Future<Output = ()> + Send;
    fn remove_follower(&self, username: &str, event_id: &str) -> impl Future<Output = ()> + Send;
    fn remove_follower_by_actor(&self, username: &str, actor: &str) -> impl Future<Output = ()> + Send;
    fn get_followers_inbox_batch(&self, username: &str, last_inbox: &str) -> impl Future<Output = (ArrayVec<String, 10>, String)> + Send;
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(tag = "event_type")]
pub enum QueueData {
    Inbox {
        username: String,
        ty: String,
        id: String,
        verified_body: Option<String>,
    },
    DeliveryNewArticleToAll {
        slug: String,
    },
    DeliveryUpdateArticleToAll {
        slug: String,
    },
    DeliveryDeleteArticleToAll {
        slug: String,
        author: String,
    },
    DeliveryNewArticleBatch {
        slug: String,
        author: String,
        last_inbox: String,
    },
    DeliveryUpdateArticleBatch {
        slug: String,
        author: String,
        last_inbox: String,
    },
    DeliveryDeleteArticleBatch {
        slug: String,
        author: String,
        last_inbox: String,
    },
    DeliveryNewArticle {
        slug: String,
        author: String,
        inbox: String,
    },
    DeliveryUpdateArticle {
        slug: String,
        author: String,
        inbox: String,
    },
    DeliveryDeleteArticle {
        slug: String,
        author: String,
        inbox: String,
    },
}

pub trait Queue {
    fn enqueue(&self, data: QueueData) -> impl Future<Output = ()> + Send;
}

pub trait HTTPClient {
    type Error: Error + Send;
    fn request(&self, request: Request<Bytes>) -> impl Future<Output = Result<axum::http::Response<Body>, Self::Error>> + Send;
}
