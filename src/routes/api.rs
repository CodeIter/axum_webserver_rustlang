//! `/api/v1` route table.

use axum::{
    http::StatusCode,
    response::Response,
    routing::get,
    Router,
};

use crate::{
    error::json_error,
    handlers::{blog, generate, redirect, request_info, system},
    middleware::headers,
    state::AppState,
};

/// JSON `404` for unknown paths under `/api/v1` (API clients expect JSON, not HTML).
async fn not_found() -> Response {
    json_error(StatusCode::NOT_FOUND, "no such API endpoint")
}

/// Builds the API router; mount it under `/api/v1`.
pub fn router() -> Router<AppState> {
    let routes = Router::new()
        .route("/hello", get(system::hello))
        .route("/ready", get(system::ready))
        .route("/health", get(system::health))
        .route("/ip", get(system::client_ip))
        .route("/blog/posts", get(blog::posts))
        .route("/http/headers", get(request_info::headers))
        .route("/http/xff", get(request_info::xff))
        .route("/number/{min}/{max}/{quantity}", get(generate::number))
        .route("/password/{length}/{quantity}", get(generate::password))
        // `{*path}` captures all remaining segments (Axum 0.8 syntax).
        .route("/redirect/{proto}/{domain}/{*path}", get(redirect::redirect))
        .fallback(not_found);

    // API responses are dynamic: never cache them.
    headers::no_store(routes)
}
