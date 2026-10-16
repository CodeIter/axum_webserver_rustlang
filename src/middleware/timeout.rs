//! Request timeout layer.

use std::time::Duration;

use axum::http::StatusCode;
use tower_http::timeout::TimeoutLayer;

/// Builds a layer that answers `408 Request Timeout` when a request exceeds `duration`.
pub fn layer(duration: Duration) -> TimeoutLayer {
    TimeoutLayer::with_status_code(StatusCode::REQUEST_TIMEOUT, duration)
}
