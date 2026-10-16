//! `/api/v1` route table.

use axum::{routing::get, Router};

use crate::{
    handlers::{generate, redirect, system},
    middleware::headers,
    state::AppState,
};

/// Builds the API router; mount it under `/api/v1`.
pub fn router() -> Router<AppState> {
    let routes = Router::new()
        .route("/hello", get(system::hello))
        .route("/ready", get(system::ready))
        .route("/health", get(system::health))
        .route("/ip", get(system::client_ip))
        .route("/number/{min}/{max}/{quantity}", get(generate::number))
        .route("/password/{length}/{quantity}", get(generate::password))
        // `{*path}` captures all remaining segments (Axum 0.8 syntax).
        .route("/redirect/{proto}/{domain}/{*path}", get(redirect::redirect));

    // API responses are dynamic: never cache them.
    headers::no_store(routes)
}
