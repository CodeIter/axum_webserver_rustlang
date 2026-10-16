//! Resolves the real client IP once per request.
//!
//! Must be the **outermost** layer: the access log, rate limiter and handlers
//! all read the resulting [`ClientIp`] from the request extensions.

use std::net::SocketAddr;

use axum::{
    extract::{ConnectInfo, Request, State},
    middleware::Next,
    response::Response,
};

use crate::services::client_ip::{self, ClientIpPolicy};

/// Stores the detected [`client_ip::ClientIp`] in the request extensions.
pub async fn resolve_client_ip(
    State(policy): State<ClientIpPolicy>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    mut request: Request,
    next: Next,
) -> Response {
    let client = client_ip::resolve(request.headers(), peer.ip(), policy);
    request.extensions_mut().insert(client);
    next.run(request).await
}
