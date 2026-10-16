//! Semaphore-based concurrency limiter.
//!
//! Unlike a queueing limiter, this one *sheds load*: when every permit is in
//! use the request is rejected immediately with `503`, so slow requests cannot
//! build an unbounded backlog and exhaust memory.

use std::sync::Arc;

use axum::{
    extract::{Request, State},
    http::{header::RETRY_AFTER, HeaderValue, StatusCode},
    middleware::Next,
    response::Response,
};
use tokio::sync::Semaphore;

use crate::error::json_error;

/// Holds a permit for the whole lifetime of the request.
pub async fn concurrency_limit(
    State(semaphore): State<Arc<Semaphore>>,
    request: Request,
    next: Next,
) -> Response {
    match semaphore.try_acquire() {
        // The permit is released when `_permit` drops, after the response is produced.
        Ok(_permit) => next.run(request).await,
        Err(_) => {
            let mut response = json_error(StatusCode::SERVICE_UNAVAILABLE, "server is at capacity");
            response
                .headers_mut()
                .insert(RETRY_AFTER, HeaderValue::from_static("1"));
            response
        }
    }
}
