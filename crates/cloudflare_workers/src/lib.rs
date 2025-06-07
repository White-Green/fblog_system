use axum::body::Body;
use axum::http::{Request, Uri};
use fblog_system_core::route::router;
use fblog_system_core::traits::*;
use futures::{Future, Stream};
use http::StatusCode;
use axum::response::IntoResponse;
use rsa::pkcs8::DecodePrivateKey;
use chrono::{DateTime, Utc};
use bytes::Bytes;
use arrayvec::ArrayVec;
use serde_json;
use http_body_util::BodyDataStream;
use worker::{event, Fetch, Context, Env, HttpRequest};
use url::Url;
use tower_service::Service;
use tower::ServiceExt;

#[derive(Clone)]
struct WorkerState {
    env: Env,
    signing_key: RSASHA2SigningKey,
}

impl WorkerState {
    fn assets(&self) -> worker::Fetcher { self.env.assets("ASSETS").unwrap() }
    fn base_url(&self) -> String { self.env.var("URL").unwrap().to_string() }

    async fn fetch_asset(&self, uri: Uri) -> http::Response<Body> {
        self.assets()
            .fetch(uri.to_string(), None)
            .await
            .map_or_else(|_| StatusCode::NOT_FOUND.into_response(), IntoResponse::into_response)
    }

    async fn fetch_bytes(&self, path: &str) -> Option<Vec<u8>> {
        let url = Url::parse(&format!("{}{}", self.base_url(), path)).ok()?;
        worker::send::SendFuture::new(async move {
            let mut resp = Fetch::Url(url).send().await.ok()?;
            if !(200..400).contains(&resp.status_code()) {
                return None;
            }
            resp.bytes().await.ok()
        })
        .await
    }
}

impl fblog_system_core::traits::Env for WorkerState {
    fn url(&self) -> impl std::fmt::Display + Send + '_ { self.env.var("URL").unwrap().to_string() }
    fn timestamp_now(&self) -> chrono::DateTime<Utc> { Utc::now() }
    fn signing_key(&self) -> &RSASHA2SigningKey { &self.signing_key }
}

impl ArticleProvider for WorkerState {
    async fn exists_article(&self, slug: &str) -> bool {
        self.fetch_bytes(&format!("/raw_ap_articles/{slug}")).await.is_some()
    }

    async fn get_article_html(&self, slug: &str) -> Option<Body> {
        self.fetch_bytes(&format!("/raw_html_articles/{slug}.html"))
            .await
            .map(Body::from)
    }

    async fn get_article_ap(&self, slug: &str) -> Option<Body> {
        self.fetch_bytes(&format!("/raw_ap_articles/{slug}"))
            .await
            .map(Body::from)
    }

    async fn get_author_id(&self, slug: &str) -> Option<String> {
        let bytes = self.fetch_bytes(&format!("/raw_ap_articles/{slug}")).await?;
        let value: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
        let attributed = value.get("attributedTo")?.as_str()?;
        Some(attributed.rsplit('/').next()?.to_string())
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
    async fn exists_user(&self, username: &str) -> bool {
        self.fetch_bytes(&format!("/raw_ap_users/{username}")).await.is_some()
    }

    async fn get_user_html(&self, _username: &str) -> Option<Body> { None }

    async fn get_user_ap(&self, username: &str) -> Option<Body> {
        self.fetch_bytes(&format!("/raw_ap_users/{username}"))
            .await
            .map(Body::from)
    }

    async fn get_followers_html(&self, _username: &str) -> Option<Body> { None }
    async fn get_followers_len(&self, _username: &str) -> usize { 0 }
    async fn get_follower_ids_until(&self, _username: &str, _until: u64) -> (ArrayVec<String,10>, u64) { (ArrayVec::new(), 0) }
    async fn add_follower(&self, _username: &str, _follower_id: String, _inbox: String) {}
    #[allow(refining_impl_trait)]
    fn get_followers_inbox(
        &self,
        _username: &str,
    ) -> impl Future<Output: Stream<Item = String> + Send> + Send {
        worker::send::SendFuture::new(async { futures::stream::empty() })
    }
}

impl Queue for WorkerState {
    async fn enqueue(&self, data: QueueData) { worker::console_log!("enqueue: {:?}", data); }
}

impl HTTPClient for WorkerState {
    type Error = reqwest::Error;
    async fn request(&self, request: Request<Bytes>) -> Result<axum::http::Response<Body>, Self::Error> {
        let req = reqwest::Request::try_from(request)?;
        let resp = reqwest::Client::new().execute(req).await?;
        let resp: axum::http::Response<reqwest::Body> = resp.into();
        let (parts, body) = resp.into_parts();
        Ok(axum::http::Response::from_parts(parts, Body::from_stream(http_body_util::BodyDataStream::new(body))))
    }
}

#[worker::send]
async fn fallback(
    uri: Uri,
    axum::extract::State(state): axum::extract::State<WorkerState>,
) -> impl IntoResponse {
    state.fetch_asset(uri).await
}

#[event(fetch)]
async fn fetch(req: HttpRequest, env: Env, _ctx: Context) -> worker::Result<http::Response<Body>> {
    console_error_panic_hook::set_once();
    let pem = env.var("PRIVATE_KEY_PEM").unwrap().to_string();
    let signing_key = RSASHA2SigningKey::from_pkcs8_pem(&pem).unwrap();
    let state = WorkerState { env: env.clone(), signing_key };
    let mut router = router(state.clone()).fallback(fallback);
    Ok(router.call(req).await?)
}
