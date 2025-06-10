use arrayvec::ArrayVec;
use axum::Json;
use axum::body::Body;
use axum::extract::{Path, Query};
use axum::http::{Request, Response, Uri};
use axum::routing::{delete, post, put};
use bytes::Bytes;
use chrono::{DateTime, Utc};
use fblog_system_core::process_queue::process_queue;
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

#[derive(Clone)]
struct Follower {
    id: String,
    inbox: String,
    event_id: String,
}

struct UserState {
    info_html: String,
    info_ap: String,
    followers: Vec<Follower>,
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
    pending_jobs: std::sync::Arc<std::sync::atomic::AtomicUsize>,
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
            pending_jobs: std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0)),
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

    #[allow(refining_impl_trait)]
    async fn get_comments_raw(&self) -> impl Stream<Item = Vec<u8>> + Send {
        let comments_raw = self.comments_raw.read().await;
        stream::iter(Vec::clone(&comments_raw).into_iter())
    }

    async fn add_comment(&self, _slug: &str, _comment: ArticleNewComment) {
        todo!()
    }

    async fn get_public_comments_until(&self, _slug: &str, until: u64) -> (ArrayVec<ArticleComment, 10>, u64) {
        let comments_raw = self.comments_raw.read().await;
        let until = (until as usize).min(comments_raw.len());
        let start = until.saturating_sub(10);
        let mut comments = ArrayVec::<ArticleComment, 10>::new();
        for raw in &comments_raw[start..until] {
            let content = String::from_utf8_lossy(raw).to_string();
            let _ = comments.try_push(ArticleComment {
                author_name: "unknown".to_owned(),
                created_at: None,
                content,
            });
        }
        (comments, start as u64)
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
        users
            .get(username)
            .map(|state| Body::from(state.followers.iter().map(|f| f.id.as_str()).collect::<Vec<_>>().join(", ")))
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
                let start = until.saturating_sub(10);
                let mut ids = ArrayVec::<String, 10>::new();
                for follower in &list[start..until] {
                    if ids.try_push(follower.id.clone()).is_err() {
                        break;
                    }
                }
                (ids, start as u64)
            }
            _ => (ArrayVec::new(), 0),
        }
    }

    async fn add_follower(&self, username: &str, follower_id: String, inbox: String, event_id: String) {
        let mut users = self.users.write().await;
        if let Some(UserState { followers, .. }) = users.get_mut(username) {
            followers.push(Follower {
                id: follower_id,
                inbox,
                event_id,
            });
        }
    }

    async fn remove_follower(&self, username: &str, event_id: String) {
        let mut users = self.users.write().await;
        if let Some(UserState { followers, .. }) = users.get_mut(username) {
            if let Some(pos) = followers.iter().position(|f| f.event_id == event_id) {
                followers.remove(pos);
            }
        }
    }

    async fn remove_follower_by_actor(&self, username: &str, actor: String) {
        let mut users = self.users.write().await;
        if let Some(UserState { followers, .. }) = users.get_mut(username) {
            if let Some(pos) = followers.iter().position(|f| f.id == actor) {
                followers.remove(pos);
            }
        }
    }

    #[allow(refining_impl_trait)]
    async fn get_followers_inbox(&self, username: &str) -> impl Stream<Item = String> + Send {
        let users = self.users.clone().read_owned().await;
        let inboxes = users.get(username).map(|user| {
            let mut unique = std::collections::HashSet::new();
            user.followers
                .iter()
                .filter_map(|f| if unique.insert(f.inbox.clone()) { Some(f.inbox.clone()) } else { None })
                .collect::<Vec<_>>()
        });
        stream::iter(inboxes.into_iter().flatten())
    }
}

impl Queue for InMemoryServer {
    async fn enqueue(&self, data: QueueData) {
        self.pending_jobs.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
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
            for entry in content_root.join("raw__").join("users").join("ap").read_dir().unwrap() {
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
                    },
                );
            }
        }
        {
            let articles_dir = content_root.join("raw__").join("articles").join("ap");
            let mut stack = vec![articles_dir.clone()];
            let mut articles = state.articles.write().await;
            while let Some(dir) = stack.pop() {
                for entry in std::fs::read_dir(&dir).unwrap() {
                    let entry = entry.unwrap();
                    let path = entry.path();
                    if entry.file_type().unwrap().is_dir() {
                        stack.push(path);
                        continue;
                    }
                    let relative = path.strip_prefix(&articles_dir).unwrap();
                    let mut without_extension = relative.to_path_buf();
                    without_extension.set_extension("");
                    let slug = without_extension.to_string_lossy().replace('\\', "/");
                    let info_ap = std::fs::read_to_string(path).unwrap();
                    articles.insert(
                        slug.clone(),
                        ArticleState {
                            author: "default".to_owned(),
                            info_html: format!("<!DOCTYPE html><html><head></head><body><h1>Article {slug}</h1></body></html>"),
                            info_ap,
                        },
                    );
                }
            }
        }
    }

    let app = router(state.clone())
        .route(
            "/job_queue",
            post({
                let pending = state.pending_jobs.clone();
                move |Json(queue_data)| {
                    pending.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    sender.send(queue_data).unwrap();
                    future::ready(())
                }
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
            "/articles/*slug",
            delete({
                let state = state.clone();
                async move |Path(slug): Path<String>| {
                    let mut articles = state.articles.write().await;
                    articles.remove(&slug);
                }
            }),
        )
        .route(
            "/job_queue_len",
            axum::routing::get({
                let pending = state.pending_jobs.clone();
                async move || {
                    let len = pending.load(std::sync::atomic::Ordering::SeqCst);
                    Json(len)
                }
            }),
        )
        .route(
            "/comments_raw",
            axum::routing::get({
                let comments = state.comments_raw.clone();
                async move || {
                    let comments = comments.read().await;
                    let body: Vec<String> = comments.iter().map(|c| String::from_utf8_lossy(c).to_string()).collect();
                    Json(body)
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
            state.pending_jobs.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
        }
    });
}
