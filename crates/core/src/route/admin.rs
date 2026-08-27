use askama::Template;
use axum::body::Body;
use axum::http::StatusCode;
use axum::http::header::CONTENT_TYPE;
use axum::response::{IntoResponse, Response};

#[derive(Template)]
#[template(
    source = r#"<!doctype html>
<html lang="ja">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>Admin</title>
</head>
<body>
  <main>
    <h1>Admin</h1>
  </main>
</body>
</html>"#,
    ext = "html"
)]
struct AdminTemplate;

#[tracing::instrument]
pub async fn admin_get() -> Response<Body> {
    match AdminTemplate.render() {
        Ok(html) => Response::builder()
            .header(CONTENT_TYPE, mime::TEXT_HTML.as_ref())
            .body(Body::from(html))
            .unwrap(),
        Err(error) => {
            tracing::error!(error = ?error, "failed to render admin dashboard");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}
