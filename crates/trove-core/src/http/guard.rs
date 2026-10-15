use std::sync::Arc;

use axum::{
    extract::{Request, State},
    http::{HeaderMap, HeaderName, StatusCode, header},
    middleware::Next,
    response::{IntoResponse, Response},
};

use super::{AppState, error::ApiError};

/// Runs before every handler. The `Host` check stops a web page from reaching the service
/// through DNS rebinding; the token stops other local programs and pages from calling it.
/// Only requests that pass count as activity, so they alone keep the service alive.
pub(super) async fn guard(
    State(state): State<Arc<AppState>>,
    request: Request,
    next: Next,
) -> Response {
    if let Err(error) = check(&state, request.headers()) {
        return error.into_response();
    }

    let _activity = state.activity.begin();

    next.run(request).await
}

fn check(state: &AppState, headers: &HeaderMap) -> Result<(), ApiError> {
    let header = |name: HeaderName| headers.get(name).and_then(|value| value.to_str().ok());

    let host_allowed = header(header::HOST)
        .is_some_and(|host| state.allowed_hosts.iter().any(|allowed| allowed == host));
    if !host_allowed {
        return Err(ApiError::new(
            StatusCode::FORBIDDEN,
            "unexpected Host header",
        ));
    }

    let token = header(header::AUTHORIZATION).and_then(|value| value.strip_prefix("Bearer "));
    if token != Some(state.runtime.token.as_str()) {
        return Err(ApiError::new(
            StatusCode::UNAUTHORIZED,
            "missing or wrong bearer token",
        ));
    }

    Ok(())
}
