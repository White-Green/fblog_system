use super::{WorkerState, admin_auth};
use axum::extract::Request as AxumRequest;
use axum::middleware::Next;
use axum::response::Response as AxumResponse;

pub(super) async fn require_admin_access(state: WorkerState, req: AxumRequest, next: Next) -> AxumResponse {
    let auth_request = match admin_auth::read_admin_auth_request(&req, &state.env) {
        Ok(auth_request) => auth_request,
        Err(error) => {
            tracing::warn!(error = %error.log_message(), "admin request rejected");
            return admin_auth::forbidden_response();
        }
    };
    if let Err(error) = admin_auth::authenticate_admin_request(auth_request).await {
        tracing::warn!(error = %error.log_message(), "admin request rejected");
        return admin_auth::forbidden_response();
    }
    next.run(req).await
}
