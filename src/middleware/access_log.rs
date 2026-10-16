//! Structured request logger.
//!
//! Emits one event per request on the [`ACCESS_LOG_TARGET`] target, which the
//! logging setup routes to stdout. Only the path is logged (never the query
//! string) so secrets passed as query parameters do not leak into logs.

use std::{net::SocketAddr, time::Instant};

use axum::{
    extract::{ConnectInfo, Request},
    middleware::Next,
    response::Response,
    Extension,
};
use tracing::info;

use crate::{logging::ACCESS_LOG_TARGET, services::client_ip::ClientIp};

/// Logs method, path, status, latency, trusted client IP and peer address.
pub async fn access_log(
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Extension(client): Extension<ClientIp>,
    request: Request,
    next: Next,
) -> Response {
    let method = request.method().clone();
    let path = request.uri().path().to_owned();
    let started = Instant::now();

    let response = next.run(request).await;

    info!(
        target: ACCESS_LOG_TARGET,
        client_ip = %client.ip,
        ip_source = client.source.as_str(),
        peer_addr = %peer.ip(),
        method = %method,
        path = %path,
        status = response.status().as_u16(),
        latency_ms = started.elapsed().as_secs_f64() * 1000.0,
        "request completed"
    );
    response
}
