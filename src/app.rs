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
/// client_ip -> access_log -> security headers -> rate limit -> concurrency limit -> timeout
///   -> templates -> API -> static files -> custom 404
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

    // Request pipeline (after the global middleware layers added below):
    //   1. template routes  (`/`, `/about`, `/blog`, ...) and `/sitemap.xml`, `/robots.txt`
    //   2. API endpoints    (`/api/v1/*`)
    //   3. static files     (fallback)
    //   4. custom 404 page  (static service's not-found fallback)
    // Matching is by exact route, so the explicit routes always win over the
    // static fallback (e.g. a file named `index.html` cannot shadow `/`).
    let router = Router::new()
        .merge(routes::pages::router())
        .merge(routes::seo::router())
        .nest("/api/v1", routes::api::router(&state))
        .fallback_service(routes::static_files::router(
            config,
            state.templates.clone(),
        ))
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
