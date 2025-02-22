use crate::common::headers::{AcceptMimeSet, HeaderReader};
use crate::traits::Env;
use axum::body::Body;
use axum::extract::State;
use axum::http::header::CONTENT_TYPE;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};

#[tracing::instrument(skip(state))]
pub async fn get_host_meta<E>(header: HeaderMap, State(state): State<E>) -> Response<Body>
where
    E: Env,
{
    let header = HeaderReader::new(&header);
    if header.select(AcceptMimeSet::XML).is_none() {
        tracing::info!("not accepted");
        return StatusCode::NOT_ACCEPTABLE.into_response();
    }
    let url = state.url();
    let body = format!(
        r#"<?xml version="1.0"?><XRD xmlns="http://docs.oasis-open.org/ns/xri/xrd-1.0"><Link rel="lrdd" type="application/xrd+xml" template="{url}/.well-known/webfinger?resource={{uri}}"/></XRD>"#
    );
    axum::http::Response::builder()
        .header(CONTENT_TYPE, mime::TEXT_XML.as_ref())
        .body(Body::from(body))
        .unwrap()
}
