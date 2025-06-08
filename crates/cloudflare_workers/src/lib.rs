use arrayvec::ArrayVec;
use axum::body::Body;
use axum::http::{Request, Uri};
use axum::response::IntoResponse;
use bytes::Bytes;
use chrono::Utc;
use fblog_system_core::route::router;
use fblog_system_core::traits::*;
use futures::stream::TryStreamExt;
use futures::{Future, Stream};
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
use worker::{Context, Env, HttpRequest, HttpResponse, console_log, event};

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
}

impl WorkerState {
    fn assets(&self) -> worker::Fetcher {
        self.env.assets("ASSETS").unwrap()
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

    async fn add_comment_raw(&self, _data: Vec<u8>) {}
    #[allow(refining_impl_trait)]
    fn get_comments_raw(&self) -> impl Future<Output = impl Stream<Item = Vec<u8>> + Send> + Send {
        futures::future::ready(futures::stream::empty())
    }
    async fn add_comment(&self, _slug: &str, _comment: ArticleNewComment) {}
    async fn get_public_comments_until(&self, _slug: &str, _until: u64) -> (ArrayVec<ArticleComment, 10>, u64) {
        (ArrayVec::new(), 0)
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

    async fn get_followers_html(&self, _username: &str) -> Option<Body> {
        None
    }
    async fn get_followers_len(&self, _username: &str) -> usize {
        0
    }
    async fn get_follower_ids_until(&self, _username: &str, _until: u64) -> (ArrayVec<String, 10>, u64) {
        (ArrayVec::new(), 0)
    }
    async fn add_follower(&self, _username: &str, _follower_id: String, _inbox: String) {}
    #[allow(refining_impl_trait)]
    fn get_followers_inbox(&self, _username: &str) -> impl Future<Output: Stream<Item = String> + Send> + Send {
        worker::send::SendFuture::new(async { futures::stream::empty() })
    }
}

impl Queue for WorkerState {
    async fn enqueue(&self, data: QueueData) {
        worker::console_log!("enqueue: {:?}", data);
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
            *builder.headers_mut().unwrap() = http::header::HeaderMap::from(mem::take(resp.headers_mut()));

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

#[event(fetch)]
async fn fetch(req: HttpRequest, env: Env, _ctx: Context) -> worker::Result<http::Response<Body>> {
    console_error_panic_hook::set_once();
    let pem = env.var("PRIVATE_KEY_PEM").unwrap().to_string();
    let signing_key = RSASHA2SigningKey::from_pkcs8_pem(&pem).unwrap();
    let state = WorkerState {
        env: env.clone(),
        signing_key,
    };
    Ok(router(state.clone()).with_state::<()>(state).call(req).await?)
}
