use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use heikas_application::error::ApplicationError;
use serde::Serialize;

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct ApiErrorBody {
    pub code: String,
    pub message: String,
    pub remedy: Option<String>,
    pub retryable: bool,
}

#[derive(Debug)]
pub struct ApiError {
    pub status: StatusCode,
    pub body: ApiErrorBody,
}

impl ApiError {
    pub fn new(status: StatusCode, code: &str, message: impl Into<String>) -> Self {
        Self {
            status,
            body: ApiErrorBody {
                code: code.to_string(),
                message: message.into(),
                remedy: None,
                retryable: false,
            },
        }
    }

    pub fn with_remedy(mut self, remedy: impl Into<String>) -> Self {
        self.body.remedy = Some(remedy.into());
        self
    }

    pub fn bad_request(message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, "invalid_request", message)
    }

    pub fn unauthorised(message: impl Into<String>) -> Self {
        Self::new(StatusCode::UNAUTHORIZED, "unauthorised", message)
    }

    pub fn forbidden(message: impl Into<String>) -> Self {
        Self::new(StatusCode::FORBIDDEN, "forbidden", message)
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self::new(StatusCode::NOT_FOUND, "not_found", message)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.status, Json(self.body)).into_response()
    }
}

impl From<ApplicationError> for ApiError {
    fn from(error: ApplicationError) -> Self {
        let status = match &error {
            ApplicationError::RunNotFound(_)
            | ApplicationError::CandidateNotFound { .. }
            | ApplicationError::ArtifactNotFound(_) => StatusCode::NOT_FOUND,
            ApplicationError::InvalidRunState { .. }
            | ApplicationError::ApprovalRequired(_)
            | ApplicationError::UserActionRequired(_) => StatusCode::CONFLICT,
            ApplicationError::InvalidConfiguration(_)
            | ApplicationError::RepositoryUnusable { .. }
            | ApplicationError::Domain(_) => StatusCode::BAD_REQUEST,
            ApplicationError::PolicyViolation(_) => StatusCode::FORBIDDEN,
            ApplicationError::RunLocked(_) => StatusCode::LOCKED,
            ApplicationError::Cancelled => StatusCode::CONFLICT,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        };
        let remedy = error.remedy();
        let retryable = matches!(
            error,
            ApplicationError::RunLocked(_) | ApplicationError::TimedOut { .. }
        );
        Self {
            status,
            body: ApiErrorBody {
                code: error.code().to_string(),
                message: error.to_string(),
                remedy,
                retryable,
            },
        }
    }
}

pub type ApiResult<T> = Result<T, ApiError>;
