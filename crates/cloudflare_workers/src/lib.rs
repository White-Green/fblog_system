use axum::body::Body;
use axum::extract::{Path, State};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Router, http};
use http::header::{ACCEPT, CONTENT_TYPE};
use http::{HeaderMap, HeaderName, StatusCode, Uri};
use tower_service::Service;
use worker::send::SendFuture;
use worker::{Bucket, Context, Env, Fetcher, HttpRequest, Var, event};

#[derive(Clone)]
struct StateType {
    env: Env,
}

impl StateType {
    fn assets(&self) -> Fetcher {
        self.env.assets("ASSETS").unwrap()
    }

    fn bucket(&self) -> Bucket {
        self.env.bucket("R2_BUCKET").unwrap()
    }

    async fn fetch_asset(&self, uri: Uri) -> Response<Body> {
        worker::console_log!("fetch asset from {:?}", uri);
        SendFuture::new(self.assets().fetch(uri.to_string(), None)).await.map_or_else(
            |e| {
                worker::console_error!("failed to fetch asset: {:?}", e);
                StatusCode::NOT_FOUND.into_response()
            },
            IntoResponse::into_response,
        )
    }

    fn url(&self) -> Var {
        self.env.var("URL").unwrap()
    }
}

fn activity_requested(headers: &HeaderMap) -> bool {
    headers.get(ACCEPT).and_then(|accept| accept.to_str().ok()).is_some_and(|accept| {
        accept.contains("application/activity+json") || accept.contains(r#"application/ld+json; profile="https://www.w3.org/ns/activitystreams""#)
    })
}

#[event(fetch)]
async fn fetch(req: HttpRequest, env: Env, _ctx: Context) -> worker::Result<http::Response<Body>> {
    worker::console_log!("{:?}", req);
    console_error_panic_hook::set_once();
    // let body = req.body_mut();
    // let mut data = Vec::new();
    // loop {
    //     let Some(frame) = std::future::poll_fn(|ctx| Pin::new(&mut *body).poll_frame(ctx)).await else {
    //         break;
    //     };
    //     let frame = frame.unwrap();
    //     data.extend(frame.data_ref().into_iter().flatten().copied());
    // }
    // worker::console_log!("body: {}", String::from_utf8_lossy(&data));
    let state = StateType { env };

    if true {
        let url = state.env.var("URL").unwrap();
        let bucket = state.bucket();
        bucket
            .put(
                "articles/a",
                format!(
                    r#"{{
  "@context":
    "https://www.w3.org/ns/activitystreams"
  ,
  "id": "{url}/articles/a",
  "type": "Article",
  "attributedTo": "{url}/users/white_green",
  "name": "ブログ記事1",
  "content": "てすと",
  "published": "2025-02-10T07:45:30.490Z",
  "to": [
    "https://www.w3.org/ns/activitystreams#Public"
  ],
  "cc": [
  ],
  "inReplyTo": null,
  "attachment": [],
  "sensitive": false,
  "tag": [
  ]
}}"#
                ),
            )
            .execute()
            .await
            .unwrap();
        bucket
            .put(
                "users/white_green",
                format!(
                    r#"{{
  "@context": [
    "https://www.w3.org/ns/activitystreams"
  ],
  "type": "Person",
  "id": "{url}/users/white_green",
  "inbox": "{url}/users/white_green/inbox",
  "outbox": "{url}/users/white_green/outbox",
  "following": "{url}/users/white_green/following",
  "followers": "{url}/users/white_green/followers",
  "preferredUsername": "white_green",
  "name": "白緑",
  "icon": {{
    "type": "Image",
    "url": "https://voskey.icalo.net/files/af3b3fb4-4fe9-47a1-8150-87b03bd85655",
    "sensitive": false,
    "name": null
  }}
}}"#
                ),
            )
            .execute()
            .await
            .unwrap();
    }

    let mut router = Router::new()
        .route("/users/:username", get(get_user))
        .route("/users/:username/inbox", post(post_user_inbox))
        .route("/users/:username/outbox", get(get_user_outbox))
        .route("/users/:username/following", get(get_user_following))
        .route("/users/:username/followers", get(get_user_followers))
        .route("/articles/:slug", get(get_article))
        .fallback(fallback_for_static_file)
        .with_state(state);
    Ok(router.call(req).await?)
}

const AP_RESULT_HEADER: [(HeaderName, &str); 1] = [(CONTENT_TYPE, "application/activity+json")];

#[worker::send]
async fn get_user(uri: Uri, header: HeaderMap, Path(username): Path<String>, State(state): State<StateType>) -> Response<Body> {
    if !activity_requested(&header) {
        return state.fetch_asset(uri).await;
    }
    let bucket = state.bucket();
    let builder = bucket.get(format!("users/{username}"));
    let result = builder.execute().await;
    let Some(object) = result.unwrap() else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let object = object.body().unwrap();
    let body = object.bytes().await.unwrap();
    worker::console_log!("body: {:?}", String::from_utf8_lossy(&body));
    (AP_RESULT_HEADER, body).into_response()
}

#[derive(Debug, serde::Deserialize)]
struct InboxData {
    id: String,
    #[serde(rename = "type")]
    ty: String,
}

#[worker::send]
async fn post_user_inbox(header: HeaderMap, Path(username): Path<String>, State(state): State<StateType>, data: String) -> Response<Body> {
    if header
        .get(CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_none_or(|v| v != "application/activity+json")
    {
        return StatusCode::BAD_REQUEST.into_response();
    }
    let Ok(data): Result<InboxData, _> = serde_json::from_str(&data) else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    worker::console_log!("data: {:?}", data);
    let client = reqwest::Client::new();
    let request = client.get(&data.id).header(
        "Accept",
        r#"application/activity+json, application/ld+json; profile="https://www.w3.org/ns/activitystreams""#,
    );
    let response = match request.send().await {
        Ok(response) => response,
        Err(e) => {
            worker::console_error!("failed to fetch: {:?}", e);
            return StatusCode::OK.into_response();
        }
    };

    #[derive(Debug, serde::Deserialize)]
    #[serde(untagged)]
    enum AnyId {
        String(String),
        Object { id: String },
    }

    #[derive(Debug, serde::Deserialize)]
    #[serde(tag = "type")]
    enum ResponseBody {
        Follow { actor: String, object: String },
        Create { actor: String, object: AnyId },
    }
    let response: ResponseBody = match response.json().await {
        Ok(response) => response,
        Err(e) => {
            worker::console_error!("failed to fetch: {:?}", e);
            return StatusCode::OK.into_response();
        }
    };
    worker::console_log!("response: {:?}", response);
    ().into_response()
}

#[worker::send]
async fn get_user_outbox(Path(username): Path<String>, State(state): State<StateType>) -> Response<Body> {
    ().into_response()
}

#[worker::send]
async fn get_user_following(header: HeaderMap, Path(username): Path<String>, State(state): State<StateType>) -> Response<Body> {
    if !activity_requested(&header) {
        return StatusCode::NOT_FOUND.into_response();
    }
    let url = state.url();
    let bucket = state.bucket();
    let builder = bucket.get(format!("users/{username}"));
    let result = builder.execute().await;
    let Some(_) = result.unwrap() else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let body = format!(
        r#"{{"@context":"https://www.w3.org/ns/activitystreams","id":{},"type":"Collection","totalItems":0,"items":[]}}"#,
        serde_json::to_string(&format!("{url}/users/{username}/following")).unwrap()
    );
    (AP_RESULT_HEADER, body).into_response()
}

#[worker::send]
async fn get_user_followers(Path(username): Path<String>, State(state): State<StateType>) -> Response<Body> {
    ().into_response()
}

#[worker::send]
async fn get_article(uri: Uri, header: HeaderMap, Path(slug): Path<String>, State(state): State<StateType>) -> Response<Body> {
    if !activity_requested(&header) {
        return state.fetch_asset(uri).await;
    }
    let bucket = state.bucket();
    let builder = bucket.get(format!("articles/{slug}"));
    let result = builder.execute().await;
    let Some(object) = result.unwrap() else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let object = object.body().unwrap();
    let body = object.bytes().await.unwrap();
    worker::console_log!("body: {:?}", String::from_utf8_lossy(&body));
    (AP_RESULT_HEADER, body).into_response()
}

async fn fallback_for_static_file(uri: Uri, State(state): State<StateType>) -> Response<Body> {
    state.fetch_asset(uri).await
}
