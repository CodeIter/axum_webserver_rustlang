//! Endpoints that echo information about the incoming request.

use std::collections::BTreeMap;

use axum::{http::HeaderMap, Json};
use serde::Serialize;

use crate::services::request_info::{self, ForwardingHeaders, HeaderValues};

#[derive(Serialize)]
pub struct HeadersResponse {
    count: usize,
    headers: BTreeMap<String, HeaderValues>,
}

/// `GET /api/v1/http/headers` - every request header, as received.
pub async fn headers(headers: HeaderMap) -> Json<HeadersResponse> {
    let headers = request_info::collect_headers(&headers);
    Json(HeadersResponse { count: headers.len(), headers })
}

/// `GET /api/v1/http/xff` - raw `CF-Connecting-IP`, `True-Client-IP` and
/// `X-Forwarded-For` values (`null` when a header is absent).
pub async fn xff(headers: HeaderMap) -> Json<ForwardingHeaders> {
    Json(request_info::forwarding_headers(&headers))
}
