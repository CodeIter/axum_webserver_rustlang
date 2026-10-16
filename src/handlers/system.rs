//! `hello`, `ready` and `health` endpoints.

use std::{sync::atomic::Ordering, time::Instant};

use axum::{extract::State, http::StatusCode, Extension, Json};
use serde_json::{json, Value};

use crate::{
    services::{
        client_ip::ClientIp,
        system::{self, HealthReport},
    },
    state::AppState,
};

/// `GET /api/v1/hello`
pub async fn hello() -> &'static str {
    "Hello World"
}

/// `GET /api/v1/ready` - readiness probe.
///
/// Returns `503` once shutdown has begun so orchestrators drain traffic.
pub async fn ready(State(state): State<AppState>) -> (StatusCode, Json<Value>) {
    if state.ready.load(Ordering::SeqCst) {
        (StatusCode::OK, Json(json!({ "status": "ready" })))
    } else {
        (StatusCode::SERVICE_UNAVAILABLE, Json(json!({ "status": "shutting_down" })))
    }
}

/// `GET /api/v1/health` - status plus system metrics.
pub async fn health(State(state): State<AppState>) -> Json<HealthReport> {
    Json(system::collect(
        Instant::now().duration_since(state.started_at),
        state.concurrency_limit,
        state.concurrency.available_permits(),
    ))
}

/// `GET /api/v1/ip` - the detected client IP and which source it came from.
pub async fn client_ip(Extension(client): Extension<ClientIp>) -> Json<ClientIp> {
    Json(client)
}
