//! Router and middleware-stack assembly.

use std::sync::Arc;

use axum::{
    middleware::{from_fn, from_fn_with_state},
    Router,
};

use crate::{
    config::Config,
    middleware::{
        access_log::access_log,
        client_ip::resolve_client_ip,
        concurrency::concurrency_limit,
        headers,
        rate_limit::{rate_limit, RateLimiter},
        timeout,
    },
    routes,
    services::client_ip::ClientIpPolicy,
    state::AppState,
};

/// Builds the fully configured application.
///
/// `Router::layer` wraps everything added so far, so layers added *later* run
/// *first*. Request flow, outermost to innermost:
///
/// ```text
/// client_ip -> access_log -> security headers -> rate limit -> concurrency limit -> timeout -> routes
/// ```
///
/// * Client-IP resolution is outermost: logger, rate limiter and handlers all use it.
/// * The logger is next so it records rejections (`429`, `503`, `408`) too.
/// * Headers are applied to every response, including rejections.
/// * Rate limiting precedes the concurrency limiter so abusive clients never
///   consume permits.
pub fn build(config: &Config, state: AppState) -> Router {
    let limiter = Arc::new(RateLimiter::new(
        config.rate_limit_max_requests,
        config.rate_limit_window(),
    ));
    limiter.spawn_janitor();

    let router = Router::new()
        .nest("/api/v1", routes::api::router())
        // Anything that is not an API route is served from the static directory.
        .fallback_service(routes::static_files::router(config))
        .with_state(state.clone());

    let router = router
        .layer(timeout::layer(config.request_timeout()))
        .layer(from_fn_with_state(state.concurrency, concurrency_limit))
        .layer(from_fn_with_state(limiter, rate_limit));

    let ip_policy = ClientIpPolicy {
        trust_proxy_headers: config.trust_proxy_headers,
        xff_proxy_depth: config.xff_proxy_depth,
    };

    headers::apply(router)
        .layer(from_fn(access_log))
        .layer(from_fn_with_state(ip_policy, resolve_client_ip))
}
