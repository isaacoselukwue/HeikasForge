use axum::extract::State;
use axum::http::{HeaderMap, Method, StatusCode};
use axum::middleware::Next;
use axum::response::Response;

use crate::error::ApiError;
use crate::session::{Session, CSRF_HEADER, SESSION_COOKIE};
use crate::state::ApiState;

pub const OPEN_PATHS: [&str; 3] = ["/api/v1/health", "/api/v1/session", "/api/v1/graph"];

pub async fn guard(
    State(state): State<ApiState>,
    request: axum::extract::Request,
    next: Next,
) -> Result<Response, ApiError> {
    let path = request.uri().path().to_string();
    if !path.starts_with("/api/") || OPEN_PATHS.contains(&path.as_str()) {
        return Ok(next.run(request).await);
    }
    let mutating = !matches!(request.method(), &Method::GET | &Method::HEAD);
    let headers = request.headers().clone();

    if mutating {
        verify_origin(&state, &headers).await?;
    }

    let session_id = read_cookie(&headers, SESSION_COOKIE);
    let csrf = headers
        .get(CSRF_HEADER)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string);

    let session: Session = state
        .sessions
        .validate(session_id.as_deref(), csrf.as_deref(), mutating)
        .await
        .map_err(|rejection| match rejection {
            crate::session::SessionRejection::RateLimited => {
                ApiError::new(StatusCode::TOO_MANY_REQUESTS, "rate_limited", rejection.message())
            }
            crate::session::SessionRejection::CsrfMismatch => {
                ApiError::forbidden(rejection.message())
            }
            other => ApiError::unauthorised(other.message())
                .with_remedy("Reload the interface so it can establish a new session."),
        })?;

    let mut request = request;
    request.extensions_mut().insert(session);
    Ok(next.run(request).await)
}

async fn verify_origin(state: &ApiState, headers: &HeaderMap) -> Result<(), ApiError> {
    let Some(expected) = state.expected_origin().await else {
        return Ok(());
    };
    let presented = headers
        .get(axum::http::header::ORIGIN)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string)
        .or_else(|| {
            headers
                .get(axum::http::header::REFERER)
                .and_then(|value| value.to_str().ok())
                .and_then(|value| {
                    let without_scheme = value.split("://").nth(1)?;
                    let authority = without_scheme.split('/').next()?;
                    let scheme = value.split("://").next()?;
                    Some(format!("{scheme}://{authority}"))
                })
        });
    match presented {
        Some(origin) if origin == expected => Ok(()),
        Some(origin) => Err(ApiError::forbidden(format!(
            "the origin `{origin}` is not permitted"
        ))),
        None => Err(ApiError::forbidden(
            "a state-changing request must declare its origin",
        )),
    }
}

pub fn read_cookie(headers: &HeaderMap, name: &str) -> Option<String> {
    let raw = headers
        .get(axum::http::header::COOKIE)
        .and_then(|value| value.to_str().ok())?;
    raw.split(';')
        .filter_map(|entry| entry.split_once('='))
        .find(|(key, _)| key.trim() == name)
        .map(|(_, value)| value.trim().to_string())
}
