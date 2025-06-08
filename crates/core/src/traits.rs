use arrayvec::ArrayVec;
use axum::body::Body;
use axum::http::Request;
use bytes::Bytes;
use chrono::{DateTime, Utc};
use futures::Stream;
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

#[derive(Debug)]
pub struct ArticleNewComment {
    pub author_id: String,
    pub author_name: String,
    pub created_at: Option<DateTime<Utc>>,
    pub content: String,
    pub raw: String,
    pub visible_in_public: bool,
}

#[derive(Debug, Serialize)]
pub struct ArticleComment {
    pub author_name: String,
    pub created_at: Option<DateTime<Utc>>,
    pub content: String,
}

pub trait ArticleProvider {
    fn exists_article(&self, slug: &str) -> impl Future<Output = bool> + Send;
    fn get_article_html(&self, slug: &str) -> impl Future<Output = Option<Body>> + Send;
    fn get_article_ap(&self, slug: &str) -> impl Future<Output = Option<Body>> + Send;
    fn get_author_id(&self, slug: &str) -> impl Future<Output = Option<String>> + Send;

    fn add_comment_raw(&self, data: Vec<u8>) -> impl Future<Output = ()> + Send;
    fn get_comments_raw(&self) -> impl Future<Output: Stream<Item = Vec<u8>>> + Send;
    fn add_comment(&self, slug: &str, comment: ArticleNewComment) -> impl Future<Output = ()> + Send;
    fn get_public_comments_until(&self, slug: &str, until: u64) -> impl Future<Output = (ArrayVec<ArticleComment, 10>, u64)> + Send;
}

pub trait UserProvider {
    fn exists_user(&self, username: &str) -> impl Future<Output = bool> + Send;
    fn get_user_html(&self, username: &str) -> impl Future<Output = Option<Body>> + Send;
    fn get_user_ap(&self, username: &str) -> impl Future<Output = Option<Body>> + Send;

    fn get_followers_html(&self, username: &str) -> impl Future<Output = Option<Body>> + Send;
    fn get_followers_len(&self, username: &str) -> impl Future<Output = usize> + Send;
    fn get_follower_ids_until(&self, username: &str, until: u64) -> impl Future<Output = (ArrayVec<String, 10>, u64)> + Send;

    fn add_follower(&self, username: &str, follower_id: String, inbox: String, event_id: String) -> impl Future<Output = ()> + Send;
    fn get_followers_inbox(&self, username: &str) -> impl Future<Output: Stream<Item = String> + Send> + Send;
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum QueueData {
    Inbox {
        username: String,
        ty: String,
        id: String,
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
    Follow {
        username: String,
        actor: String,
        object: String,
        id: String,
    },
}

pub trait Queue {
    fn enqueue(&self, data: QueueData) -> impl Future<Output = ()> + Send;
}

pub trait HTTPClient {
    type Error: Error + Send;
    fn request(&self, request: Request<Bytes>) -> impl Future<Output = Result<axum::http::Response<Body>, Self::Error>> + Send;
}
