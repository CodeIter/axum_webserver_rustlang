//! Error types and uniform JSON error responses.

use axum::{
    extract::rejection::{JsonRejection, PathRejection, QueryRejection},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;
use tracing::{error, warn};

use crate::services::store::StoreError;

/// Builds a JSON error body: `{"error": "...", "status": 429}`.
///
/// Shared by handlers and middleware so every error looks the same.
pub fn json_error(status: StatusCode, message: impl Into<String>) -> Response {
    let body = json!({ "error": message.into(), "status": status.as_u16() });
    (status, Json(body)).into_response()
}

/// Errors returned by API handlers and services.
#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    /// The client sent an invalid request (`400`).
    #[error("{0}")]
    BadRequest(String),
    /// Unexpected failure; details are logged, never sent to the client (`500`).
    #[error("internal server error")]
    Internal,
    /// The requested resource does not exist (`404`).
    #[error("{0}")]
    NotFound(String),
    /// The request conflicts with existing state (`409`).
    #[error("{0}")]
    Conflict(String),
    /// A backing service is unavailable or not configured (`503`).
    #[error("{0}")]
    ServiceUnavailable(String),
    /// A backing store is full (`507`).
    #[error("{0}")]
    InsufficientStorage(String),
    /// A backing service answered with an error (`502`); details are only logged.
    #[error("bad gateway")]
    BadGateway,
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        match self {
            Self::BadRequest(msg) => json_error(StatusCode::BAD_REQUEST, msg),
            Self::Internal => {
                json_error(StatusCode::INTERNAL_SERVER_ERROR, "internal server error")
            }
            Self::NotFound(msg) => json_error(StatusCode::NOT_FOUND, msg),
            Self::Conflict(msg) => json_error(StatusCode::CONFLICT, msg),
            Self::ServiceUnavailable(msg) => json_error(StatusCode::SERVICE_UNAVAILABLE, msg),
            Self::InsufficientStorage(msg) => json_error(StatusCode::INSUFFICIENT_STORAGE, msg),
            Self::BadGateway => json_error(StatusCode::BAD_GATEWAY, "upstream database error"),
        }
    }
}

// Convert extractor rejections into our JSON error format.
impl From<PathRejection> for ApiError {
    fn from(rejection: PathRejection) -> Self {
        Self::BadRequest(rejection.to_string())
    }
}

impl From<QueryRejection> for ApiError {
    fn from(rejection: QueryRejection) -> Self {
        Self::BadRequest(rejection.to_string())
    }
}

impl From<JsonRejection> for ApiError {
    fn from(rejection: JsonRejection) -> Self {
        Self::BadRequest(rejection.body_text())
    }
}

/// Maps store failures to HTTP errors. Backend details are logged, never returned.
impl From<StoreError> for ApiError {
    fn from(err: StoreError) -> Self {
        match err {
            StoreError::Invalid(message) => Self::BadRequest(message),
            StoreError::NotFound => Self::NotFound("key not found".into()),
            StoreError::AlreadyExists => Self::Conflict("key already exists".into()),
            StoreError::CapacityExceeded => Self::InsufficientStorage("store is full".into()),
            StoreError::Unavailable(detail) => {
                warn!(%detail, "store unavailable");
                Self::ServiceUnavailable("database is unavailable".into())
            }
            StoreError::Backend(detail) => {
                error!(%detail, "store backend error");
                Self::BadGateway
            }
        }
    }
}
