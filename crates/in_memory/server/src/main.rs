use arrayvec::ArrayVec;
use axum::Json;
use axum::body::Body;
use axum::extract::{Path, Query};
use axum::http::{Request, Response, Uri};
use axum::routing::{delete, get, post, put};
use bytes::Bytes;
use chrono::{DateTime, Utc};
use fblog_system_core::process_queue::{ProcessQueueResult, process_queue};
use fblog_system_core::route::router;
use fblog_system_core::traits::{ArticleComment, ArticleNewComment, ArticleProvider, Env, HTTPClient, Queue, QueueData, UserProvider};
use futures::{Stream, stream};
use rsa::pkcs1v15::SigningKey;
use rsa::pkcs8::DecodePrivateKey;
use rsa::sha2::Sha256;
use serde::Deserialize;
use std::collections::HashMap;
use std::fmt::Display;
use std::sync::Arc;
use std::{env, future};
use tokio::sync::RwLock as TokioRwLock;
use tower_http::trace::{self, TraceLayer};
use tracing::Level;

struct UserState {
    info_html: String,
    info_ap: String,
    followers: Vec<String>,
    followers_inbox: Vec<String>,
}

#[derive(Debug)]
struct ArticleState {
    author: String,
    info_html: String,
    info_ap: String,
}

#[derive(Clone)]
struct InMemoryServer {
    articles: Arc<TokioRwLock<HashMap<String, ArticleState>>>,
    comments_raw: Arc<TokioRwLock<Vec<Vec<u8>>>>,
    users: Arc<TokioRwLock<HashMap<String, UserState>>>,
    queue: tokio::sync::mpsc::UnboundedSender<QueueData>,
    client: reqwest::Client,
    base_url: String,
    key: SigningKey<rsa::sha2::Sha256>,
}

impl InMemoryServer {
    fn new(queue: tokio::sync::mpsc::UnboundedSender<QueueData>, key: SigningKey<rsa::sha2::Sha256>) -> Self {
        let mut client_builder = reqwest::ClientBuilder::new()
            .resolve("misskey.test", "127.0.0.1:443".parse().unwrap())
            .resolve("mastodon.test", "127.0.0.1:443".parse().unwrap())
            .resolve("sharkey.test", "127.0.0.1:443".parse().unwrap());
        if let Ok(certificate_path) = env::var("ADDITIONAL_CERTIFICATE_PEM") {
            let certificate_pem = std::fs::read(dbg!(certificate_path)).unwrap();
            let certificate = reqwest::Certificate::from_pem(&certificate_pem).unwrap();
            client_builder = client_builder.add_root_certificate(certificate);
        }

        Self {
            articles: Arc::new(TokioRwLock::new(HashMap::new())),
            comments_raw: Arc::new(TokioRwLock::new(Vec::new())),
            users: Arc::new(TokioRwLock::new(HashMap::new())),
            queue,
            client: client_builder.build().unwrap(),
            base_url: "https://blog.test".to_string(),
            key,
        }
    }
}

impl Env for InMemoryServer {
    fn url(&self) -> impl Display + Send + '_ {
        self.base_url.as_str()
    }

    fn timestamp_now(&self) -> DateTime<Utc> {
        Utc::now()
    }

    fn signing_key(&self) -> &SigningKey<rsa::sha2::Sha256> {
        &self.key
    }
}

impl ArticleProvider for InMemoryServer {
    async fn exists_article(&self, slug: &str) -> bool {
        let articles = self.articles.read().await;
        articles.contains_key(slug)
    }

    async fn get_article_html(&self, slug: &str) -> Option<Body> {
        let articles = self.articles.read().await;
        articles.get(slug).map(|state| Body::from(state.info_html.clone()))
    }

    async fn get_article_ap(&self, slug: &str) -> Option<Body> {
        let articles = self.articles.read().await;
        articles.get(slug).map(|state| Body::from(state.info_ap.clone()))
    }

    async fn get_author_id(&self, slug: &str) -> Option<String> {
        let articles = self.articles.read().await;
        tracing::info!("{articles:?}");
        articles.get(slug).map(|state| state.author.clone())
    }

    async fn add_comment_raw(&self, data: Vec<u8>) {
        let mut comments_raw = self.comments_raw.write().await;
        comments_raw.push(data);
    }

    async fn get_comments_raw(&self) -> impl Stream<Item = Vec<u8>> + Send {
        let comments_raw = self.comments_raw.read().await;
        stream::iter(Vec::clone(&comments_raw).into_iter())
    }

    async fn add_comment(&self, slug: &str, comment: ArticleNewComment) {
        todo!()
    }

    async fn get_public_comments_until(&self, slug: &str, until: u64) -> (ArrayVec<ArticleComment, 10>, u64) {
        todo!()
    }
}

impl UserProvider for InMemoryServer {
    async fn exists_user(&self, username: &str) -> bool {
        let users = self.users.read().await;
        users.contains_key(username)
    }

    async fn get_user_html(&self, username: &str) -> Option<Body> {
        let users = self.users.read().await;
        users.get(username).map(|state| Body::from(state.info_html.clone()))
    }

    async fn get_user_ap(&self, username: &str) -> Option<Body> {
        let users = self.users.read().await;
        users.get(username).map(|state| Body::from(state.info_ap.clone()))
    }

    async fn get_followers_html(&self, username: &str) -> Option<Body> {
        let users = self.users.read().await;
        users.get(username).map(|state| Body::from(state.followers.join(", ")))
    }

    async fn get_followers_len(&self, username: &str) -> usize {
        let users = self.users.read().await;
        users.get(username).map(|state| state.followers.len()).unwrap_or(0)
    }

    async fn get_follower_ids_until(&self, username: &str, until: u64) -> (ArrayVec<String, 10>, u64) {
        let map = self.users.read().await;
        match map.get(username) {
            Some(UserState { followers: list, .. }) => {
                let until = list.len().min(until as usize);
                let list = &list[..until];
                let next_ts = list.len().saturating_sub(10);
                (list[next_ts..].try_into().unwrap(), next_ts as u64)
            }
            _ => (<&[String]>::try_into(&[]).unwrap(), 0),
        }
    }

    async fn add_follower(&self, username: &str, follower_id: String, inbox: String) {
        let mut users = self.users.write().await;
        if let Some(UserState {
            followers, followers_inbox, ..
        }) = users.get_mut(username)
        {
            followers.push(follower_id);
            if !followers_inbox.iter().any(|x| x == &inbox) {
                followers_inbox.push(inbox);
            }
        }
    }

    async fn get_followers_inbox(&self, username: &str) -> impl Stream<Item = String> + Send {
        let users = self.users.clone().read_owned().await;
        let followers_inbox = users.get(username).map(|user| user.followers_inbox.clone());
        stream::iter(followers_inbox.map(|followers_inbox| followers_inbox.into_iter()).into_iter().flatten())
    }
}

impl Queue for InMemoryServer {
    async fn enqueue(&self, data: QueueData) {
        self.queue.send(data).unwrap();
    }
}

impl HTTPClient for InMemoryServer {
    type Error = reqwest::Error;
    async fn request(&self, request: Request<Bytes>) -> Result<Response<Body>, Self::Error> {
        let request = reqwest::Request::try_from(request)?;
        let response = self.client.execute(request).await?;
        let response: Response<reqwest::Body> = response.into();
        let (parts, body) = response.into_parts();
        Ok(Response::from_parts(parts, Body::from_stream(http_body_util::BodyDataStream::new(body))))
    }
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let signing_key = SigningKey::<Sha256>::from_pkcs8_pem(include_str!("../../../../test_config/private-key-for-test.pem")).unwrap();
    let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel();
    let state = InMemoryServer::new(sender.clone(), signing_key);
    {
        let content_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("dist");
        {
            let mut users = state.users.write().await;
            for entry in content_root.join("users").read_dir().unwrap() {
                let entry = entry.unwrap();
                assert!(entry.file_type().unwrap().is_file());
                let without_extension = entry.path().with_extension("");
                let username = without_extension.file_name().unwrap();
                let username = username.to_str().unwrap();
                let info_ap = std::fs::read_to_string(entry.path()).unwrap();
                users.insert(
                    username.to_owned(),
                    UserState {
                        info_html: format!("<!DOCTYPE html><html><head></head><body><h1>{username}'s UserPage</h1></body></html>"),
                        info_ap,
                        followers: vec![],
                        followers_inbox: vec![],
                    },
                );
            }
        }
        {
            let mut articles = state.articles.write().await;
            for entry in content_root.join("articles").read_dir().unwrap() {
                let entry = entry.unwrap();
                assert!(entry.file_type().unwrap().is_file());
                let without_extension = entry.path().with_extension("");
                let slug = without_extension.file_name().unwrap();
                let slug = slug.to_str().unwrap();
                let info_ap = std::fs::read_to_string(entry.path()).unwrap();
                articles.insert(
                    slug.to_owned(),
                    ArticleState {
                        author: "default".to_owned(),
                        info_html: format!("<!DOCTYPE html><html><head></head><body><h1>Article {slug}</h1></body></html>"),
                        info_ap,
                    },
                );
            }
        }
    }

    let app = router(state.clone())
        .route(
            "/job_queue",
            post(move |Json(queue_data)| {
                sender.send(queue_data).unwrap();
                future::ready(())
            }),
        )
        .route(
            "/article_ap",
            put({
                let state = state.clone();
                #[derive(Deserialize)]
                struct ReplaceArticleQuery {
                    slug: String,
                }
                async move |Query(ReplaceArticleQuery { slug }), body: String| {
                    let mut articles = state.articles.write().await;
                    articles.get_mut(&slug).unwrap().info_ap = body;
                }
            }),
        )
        .route(
            "/articles/:slug",
            delete({
                let state = state.clone();
                async move |Path(slug): Path<String>| {
                    let mut articles = state.articles.write().await;
                    articles.remove(&slug);
                }
            }),
        )
        .fallback(|uri: Uri| {
            tracing::trace!("fallback; uri: {uri:?}");
            let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .parent()
                .unwrap()
                .parent()
                .unwrap()
                .parent()
                .unwrap()
                .join("dist")
                .join(uri.path().strip_prefix("/").unwrap_or(uri.path()));
            match std::fs::metadata(&path) {
                Ok(meta) if meta.is_file() => match std::fs::read(&path) {
                    Ok(file) => future::ready(Ok(file)),
                    Err(e) => {
                        tracing::error!("Error {e}");
                        future::ready(Err(Response::builder().status(500).body(Body::empty()).unwrap()))
                    }
                },
                Ok(_) => future::ready(Err(Response::builder().status(404).body(Body::empty()).unwrap())),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    future::ready(Err(Response::builder().status(404).body(Body::empty()).unwrap()))
                }
                Err(e) => {
                    tracing::error!("Error {e}");
                    future::ready(Err(Response::builder().status(500).body(Body::empty()).unwrap()))
                }
            }
        })
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(trace::DefaultMakeSpan::new().level(Level::INFO))
                .on_response(trace::DefaultOnResponse::new().level(Level::INFO)),
        );

    let listener = tokio::net::TcpListener::bind("0.0.0.0:8787").await.unwrap();
    tokio::join!(axum::serve(listener, app), async move {
        loop {
            let data = receiver.recv().await.unwrap();
            process_queue(&state, data).await;
        }
    });
}
