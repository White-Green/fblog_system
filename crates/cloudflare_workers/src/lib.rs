use arrayvec::ArrayVec;
use axum::body::Body;
use axum::http::Request;
use axum::response::IntoResponse;
use bytes::Bytes;
use chrono::Utc;
use fblog_system_core::process_queue::{ProcessQueueResult, process_queue};
use fblog_system_core::route::router;
use fblog_system_core::traits::*;
use futures::Stream;
use futures::stream::TryStreamExt;
use http::StatusCode;
use http_body_util::{BodyDataStream, BodyExt};
use rsa::pkcs8::DecodePrivateKey;
use std::fmt::Display;
use std::mem;
use std::pin::Pin;
use std::task::{Context as TaskContext, Poll};
use tower_service::Service;
use tracing_subscriber::fmt::format::Pretty;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_web::MakeConsoleWriter;
use worker::{Context, Env, HttpRequest, MessageExt, event};

#[cfg(feature = "test")]
mod tests;

struct SendStream<S> {
    inner: S,
}

unsafe impl<S> Send for SendStream<S> {}

impl<S: Stream + Unpin> Stream for SendStream<S> {
    type Item = S::Item;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut TaskContext<'_>) -> Poll<Option<Self::Item>> {
        Pin::new(&mut self.inner).poll_next(cx)
    }
}

#[derive(Clone)]
struct WorkerState {
    env: Env,
    signing_key: RSASHA2SigningKey,
    queue: worker::Queue,
    db: std::sync::Arc<worker::d1::D1Database>,
}

impl WorkerState {
    fn assets(&self) -> worker::Fetcher {
        self.env.assets("ASSETS").unwrap()
    }

    fn r2(&self) -> worker::Bucket {
        self.env.bucket("R2_BUCKET").unwrap()
    }

    #[worker::send]
    async fn fetch_asset(&self, path: impl Display) -> http::Response<Body> {
        self.assets()
            .fetch(format!("http://localhost{path}"), None)
            .await
            .map_or_else(|_| StatusCode::NOT_FOUND.into_response(), IntoResponse::into_response)
    }
}

impl fblog_system_core::traits::Env for WorkerState {
    fn url(&self) -> impl std::fmt::Display + Send + '_ {
        self.env.var("URL").unwrap().to_string()
    }
    fn timestamp_now(&self) -> chrono::DateTime<Utc> {
        Utc::now()
    }
    fn signing_key(&self) -> &RSASHA2SigningKey {
        &self.signing_key
    }
}

impl ArticleProvider for WorkerState {
    #[worker::send]
    async fn exists_article(&self, slug: &str) -> bool {
        tracing::trace!("exists_article: {}", slug);
        self.fetch_asset(format_args!("/raw__/articles/ap/{slug}.json"))
            .await
            .status()
            .is_success()
    }

    #[worker::send]
    async fn get_article_html(&self, slug: &str) -> Option<Body> {
        tracing::trace!("get_article_html: {}", slug);
        let response = self.fetch_asset(format_args!("/raw__/articles/html/{slug}.html")).await;
        if response.status().is_success() {
            Some(Body::from_stream(BodyDataStream::new(response.into_body())))
        } else {
            None
        }
    }

    #[worker::send]
    async fn get_article_ap(&self, slug: &str) -> Option<Body> {
        tracing::trace!("get_article_ap: {}", slug);
        let response = self.fetch_asset(format_args!("/raw__/articles/ap/{slug}.json")).await;
        if response.status().is_success() {
            Some(Body::from_stream(BodyDataStream::new(response.into_body())))
        } else {
            None
        }
    }

    #[worker::send]
    async fn get_author_id(&self, slug: &str) -> Option<String> {
        let response = self.fetch_asset(format_args!("/raw__/articles/author/{slug}")).await;
        if !response.status().is_success() {
            return None;
        }
        let body = BodyExt::collect(response.into_body()).await.ok()?;
        let data = body.to_bytes().to_vec();
        let author_id = String::from_utf8(data).ok()?;
        Some(author_id)
    }

    #[worker::send]
    async fn add_comment(&self, slug: &str, comment: ArticleNewComment) {
        // Serialize the comment and store it in R2 bucket
        let json = match serde_json::to_string(&comment) {
            Ok(json) => json,
            Err(e) => {
                tracing::error!(error = ?e, "Failed to serialize comment");
                return;
            }
        };

        let path = format!("comments/{}/{}", slug, comment.id);
        if let Err(e) = self.r2().put(&path, json.clone()).execute().await {
            tracing::error!(error = ?e, "Failed to store comment in R2");
            return;
        }

        // Increment the comment count in D1
        match worker::query!(
            self.db.as_ref(),
            "INSERT INTO comments (slug, count) VALUES (?1, 1)\
             ON CONFLICT (slug) DO UPDATE SET count = count + 1",
            &slug
        ) {
            Ok(stmt) => {
                if let Err(e) = stmt.run().await {
                    tracing::error!(error = ?e, "Failed to increment comment count in D1");
                }
            }
            Err(e) => {
                tracing::error!(error = ?e, "Failed to prepare increment comment count query");
            }
        }
    }

    #[worker::send]
    async fn remove_reaction_by(&self, slug: &str, actor: &str) {
        // D1からリアクションのカウントを減らす
        match worker::query!(
            self.db.as_ref(),
            "UPDATE reactions SET count = CASE WHEN count > 0 THEN count - 1 ELSE 0 END WHERE slug = ?1",
            &slug
        ) {
            Ok(stmt) => {
                if let Err(e) = stmt.run().await {
                    tracing::error!(error = ?e, "Failed to decrement reaction count in D1");
                }
            }
            Err(e) => {
                tracing::error!(error = ?e, "Failed to prepare decrement reaction count query");
            }
        }

        // D1からアクターのリアクション情報を削除
        match worker::query!(
            self.db.as_ref(),
            "DELETE FROM reaction_actors WHERE slug = ?1 AND actor_id = ?2",
            &slug,
            &actor
        ) {
            Ok(stmt) => {
                if let Err(e) = stmt.run().await {
                    tracing::error!(error = ?e, "Failed to remove actor's reaction from D1");
                }
            }
            Err(e) => {
                tracing::error!(error = ?e, "Failed to prepare remove actor's reaction query");
            }
        }
    }

    #[worker::send]
    async fn add_reaction(&self, slug: &str, reaction: ArticleNewReaction) {
        // Serialize the reaction and store it in R2 bucket
        let json = match serde_json::to_string(&reaction) {
            Ok(json) => json,
            Err(e) => {
                tracing::error!(error = ?e, "Failed to serialize reaction");
                return;
            }
        };

        let path = format!("reactions/{}/{}", slug, reaction.id);
        if let Err(e) = self.r2().put(&path, json.clone()).execute().await {
            tracing::error!(error = ?e, "Failed to store reaction in R2");
            return;
        }

        // Increment the reaction count in D1
        match worker::query!(
            self.db.as_ref(),
            "INSERT INTO reactions (slug, count) VALUES (?1, 1)\
             ON CONFLICT (slug) DO UPDATE SET count = count + 1",
            &slug
        ) {
            Ok(stmt) => {
                if let Err(e) = stmt.run().await {
                    tracing::error!(error = ?e, "Failed to increment reaction count in D1");
                }
            }
            Err(e) => {
                tracing::error!(error = ?e, "Failed to prepare increment reaction count query");
            }
        }

        // リアクションを行ったアクターの情報をD1に保存
        match worker::query!(
            self.db.as_ref(),
            "INSERT OR REPLACE INTO reaction_actors (slug, actor_id, reaction_id) VALUES (?1, ?2, ?3)",
            &slug,
            &reaction.author_id,
            &reaction.id
        ) {
            Ok(stmt) => {
                if let Err(e) = stmt.run().await {
                    tracing::error!(error = ?e, "Failed to store actor's reaction in D1");
                }
            }
            Err(e) => {
                tracing::error!(error = ?e, "Failed to prepare store actor's reaction query");
            }
        }
    }

    #[worker::send]
    async fn comment_count(&self, slug: &str) -> usize {
        let stmt = match worker::query!(self.db.as_ref(), "SELECT count FROM comments WHERE slug = ?1", &slug) {
            Ok(s) => s,
            Err(e) => {
                tracing::error!(error = ?e, "Failed to prepare comment count query");
                return 0;
            }
        };

        match stmt.first::<i64>(Some("count")).await {
            Ok(Some(count)) => count as usize,
            Ok(None) => 0, // No record found
            Err(e) => {
                tracing::error!(error = ?e, "Failed to execute comment count query");
                0
            }
        }
    }

    #[worker::send]
    async fn reaction_count(&self, slug: &str) -> usize {
        let stmt = match worker::query!(self.db.as_ref(), "SELECT count FROM reactions WHERE slug = ?1", &slug) {
            Ok(s) => s,
            Err(e) => {
                tracing::error!(error = ?e, "Failed to prepare reaction count query");
                return 0;
            }
        };

        match stmt.first::<i64>(Some("count")).await {
            Ok(Some(count)) => count as usize,
            Ok(None) => 0, // No record found
            Err(e) => {
                tracing::error!(error = ?e, "Failed to execute reaction count query");
                0
            }
        }
    }
}

impl UserProvider for WorkerState {
    #[worker::send]
    async fn exists_user(&self, username: &str) -> bool {
        self.fetch_asset(format_args!("/raw__/users/ap/{username}.json"))
            .await
            .status()
            .is_success()
    }

    #[worker::send]
    async fn get_user_html(&self, username: &str) -> Option<Body> {
        let response = self.fetch_asset(format_args!("/raw__/users/html/{username}.html")).await;
        if response.status().is_success() {
            Some(Body::from_stream(BodyDataStream::new(response.into_body())))
        } else {
            None
        }
    }

    #[worker::send]
    async fn get_user_ap(&self, username: &str) -> Option<Body> {
        let response = self.fetch_asset(format_args!("/raw__/users/ap/{username}.json")).await;
        if response.status().is_success() {
            Some(Body::from_stream(BodyDataStream::new(response.into_body())))
        } else {
            None
        }
    }

    #[worker::send]
    async fn add_follower(&self, username: &str, follower_id: &str, inbox: &str, event_id: &str) {
        match worker::query!(
            self.db.as_ref(),
            "INSERT INTO followers (username, follower_id, inbox, event_id) VALUES (?1, ?2, ?3, ?4)",
            &username,
            &follower_id,
            &inbox,
            &event_id,
        ) {
            Ok(stmt) => {
                if let Err(e) = stmt.run().await {
                    tracing::error!(error = ?e, "failed to insert follower");
                }
            }
            Err(e) => {
                tracing::error!(error = ?e, "failed to prepare insert follower");
            }
        }
    }

    #[worker::send]
    async fn remove_follower(&self, username: &str, event_id: &str) {
        match worker::query!(
            self.db.as_ref(),
            "DELETE FROM followers WHERE username = ?1 AND event_id = ?2",
            &username,
            &event_id,
        ) {
            Ok(stmt) => {
                if let Err(e) = stmt.run().await {
                    tracing::error!(error = ?e, "failed to delete follower");
                }
            }
            Err(e) => {
                tracing::error!(error = ?e, "failed to prepare delete follower");
            }
        }
    }

    #[worker::send]
    async fn remove_follower_by_actor(&self, username: &str, actor: &str) {
        match worker::query!(
            self.db.as_ref(),
            "DELETE FROM followers WHERE username = ?1 AND follower_id = ?2",
            &username,
            &actor,
        ) {
            Ok(stmt) => {
                if let Err(e) = stmt.run().await {
                    tracing::error!(error = ?e, "failed to delete follower by actor");
                }
            }
            Err(e) => {
                tracing::error!(error = ?e, "failed to prepare delete follower by actor");
            }
        }
    }

    #[worker::send]
    async fn get_followers_inbox_batch(&self, username: &str, last_inbox: &str) -> (ArrayVec<String, 10>, String) {
        let stmt = match worker::query!(
            self.db.as_ref(),
            "SELECT DISTINCT inbox FROM followers WHERE username = ?1 AND inbox > ?2 ORDER BY inbox LIMIT 10",
            &username,
            &last_inbox
        ) {
            Ok(s) => s,
            Err(e) => {
                tracing::error!(error = ?e, "failed to prepare get_followers_inbox_batch");
                return (ArrayVec::new(), last_inbox.to_string());
            }
        };
        let rows: Vec<Vec<String>> = match stmt.raw().await {
            Ok(r) => r,
            Err(e) => {
                tracing::error!(error = ?e, "failed to execute get_followers_inbox_batch");
                return (ArrayVec::new(), last_inbox.to_string());
            }
        };
        let mut vec = ArrayVec::<String, 10>::new();
        for mut row in rows {
            if let Some(inbox) = row.pop() {
                if vec.try_push(inbox).is_err() {
                    break;
                }
            }
        }
        let next_last = vec.last().cloned().unwrap_or_default();
        (vec, next_last)
    }
}
impl Queue for WorkerState {
    async fn enqueue(&self, data: QueueData) {
        worker::send::SendFuture::new(async move {
            if let Err(e) = self.queue.send(data).await {
                worker::console_error!("failed to enqueue: {:?}", e);
            }
        })
        .await
    }
}

impl HTTPClient for WorkerState {
    type Error = reqwest::Error;
    async fn request(&self, request: Request<Bytes>) -> Result<axum::http::Response<Body>, Self::Error> {
        let req = reqwest::Request::try_from(request)?;
        worker::send::SendFuture::new(async move {
            let mut resp = reqwest::Client::new().execute(req).await?;

            let status = resp.status();
            let mut builder = axum::http::Response::builder().status(status);
            *builder.headers_mut().unwrap() = mem::take(resp.headers_mut());

            let stream = SendStream { inner: resp.bytes_stream() };
            let body = Body::from_stream(stream.map_err(axum::Error::new));

            Ok(builder.body(body).expect("failed to build response"))
        })
        .await
    }
}

#[event(start)]
fn start() {
    let fmt_layer = tracing_subscriber::fmt::layer()
        .with_ansi(false)
        .without_time()
        .with_writer(MakeConsoleWriter);
    let perf_layer = tracing_web::performance_layer().with_details_from_fields(Pretty::default());
    tracing_subscriber::registry().with(fmt_layer).with(perf_layer).init();
}

// Setup function to create WorkerState from environment
fn setup_worker_state(env: &Env) -> worker::Result<WorkerState> {
    console_error_panic_hook::set_once();
    let pem = env.var("PRIVATE_KEY_PEM").unwrap().to_string();
    let signing_key = RSASHA2SigningKey::from_pkcs8_pem(&pem).unwrap();
    let queue = env.queue("JOB_QUEUE")?;
    let db = std::sync::Arc::new(env.d1("BLOG_DB")?);
    Ok(WorkerState {
        env: env.clone(),
        signing_key,
        queue,
        db,
    })
}

#[cfg(not(feature = "test"))]
#[event(fetch)]
async fn fetch(req: HttpRequest, env: Env, _ctx: Context) -> worker::Result<http::Response<Body>> {
    let state = setup_worker_state(&env)?;
    Ok(router(state.clone()).with_state::<()>(state).call(req).await?)
}

#[cfg(feature = "test")]
#[event(fetch)]
async fn fetch(req: HttpRequest, env: Env, _ctx: Context) -> worker::Result<http::Response<Body>> {
    let url = req.uri();
    let path = url.path();
    let state = setup_worker_state(&env)?;

    // Health check endpoint - always returns 200 OK
    if path == "/" {
        return Ok(http::Response::builder().status(StatusCode::OK).body(Body::from("OK")).unwrap());
    }

    // Test endpoint
    if path == "/test" {
        tests::run_all_tests(state).await;
        return Ok(http::Response::builder()
            .status(StatusCode::OK)
            .body(Body::from("Test endpoint"))
            .unwrap());
    }

    // For any other path, return 404
    Ok(http::Response::builder()
        .status(StatusCode::NOT_FOUND)
        .body(Body::from("Not Found"))
        .unwrap())
}

#[event(queue)]
async fn queue_event(batch: worker::MessageBatch<QueueData>, env: Env, _ctx: Context) -> worker::Result<()> {
    let state = setup_worker_state(&env)?;
    for message in batch.messages()? {
        let data = message.body().clone();
        match process_queue(&state, data).await {
            ProcessQueueResult::Finished => message.ack(),
            ProcessQueueResult::Retry => message.retry(),
        }
    }
    Ok(())
}
