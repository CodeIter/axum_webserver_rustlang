//! Standard security and content headers.
//!
//! Values are deliberately hard-coded (not configurable). Headers are only set
//! when a handler has not already provided them (`if_not_present`).

use axum::{
    http::{header::CACHE_CONTROL, HeaderName, HeaderValue},
    Router,
};
use tower_http::set_header::SetResponseHeaderLayer;

/// Headers added to every response.
const SECURITY_HEADERS: &[(&str, &str)] = &[
    // Disable MIME sniffing.
    ("x-content-type-options", "nosniff"),
    // Forbid framing (clickjacking protection).
    ("x-frame-options", "DENY"),
    ("referrer-policy", "no-referrer"),
    (
        "content-security-policy",
        "default-src 'self'; style-src 'self'; img-src 'self' https: data:; \
         base-uri 'self'; form-action 'self'; frame-ancestors 'none'",
    ),
    // Ignored over plain HTTP; effective behind TLS termination.
    (
        "strict-transport-security",
        "max-age=31536000; includeSubDomains",
    ),
    (
        "permissions-policy",
        "camera=(), microphone=(), geolocation=()",
    ),
    ("cross-origin-opener-policy", "same-origin"),
    ("cross-origin-resource-policy", "same-origin"),
    ("x-permitted-cross-domain-policies", "none"),
];

/// Applies the security headers to every route (and fallback) of `router`.
pub fn apply<S>(mut router: Router<S>) -> Router<S>
where
    S: Clone + Send + Sync + 'static,
{
    for (name, value) in SECURITY_HEADERS {
        router = router.layer(SetResponseHeaderLayer::if_not_present(
            HeaderName::from_static(name),
            HeaderValue::from_static(value),
        ));
    }
    router
}

/// Marks responses as non-cacheable (used for dynamic API responses).
pub fn no_store<S>(router: Router<S>) -> Router<S>
where
    S: Clone + Send + Sync + 'static,
{
    router.layer(SetResponseHeaderLayer::if_not_present(
        CACHE_CONTROL,
        HeaderValue::from_static("no-store"),
    ))
}
