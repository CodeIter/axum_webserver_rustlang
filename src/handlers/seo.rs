//! `/sitemap.xml` and `/robots.txt`, generated on every request.

use axum::{
    extract::State,
    http::{
        header::{CACHE_CONTROL, CONTENT_TYPE},
        HeaderMap, StatusCode, Uri,
    },
    response::{IntoResponse, Response},
};
use tracing::error;

use crate::{routes::pages::PAGE_PATHS, services::seo, state::AppState};

/// Crawlers may cache these for an hour.
const CACHE: &str = "public, max-age=3600";

/// `GET /sitemap.xml` - template pages, blog posts and static HTML files.
pub async fn sitemap(State(state): State<AppState>, headers: HeaderMap, uri: Uri) -> Response {
    let base = state.site.base_url(&headers, &uri);

    // Directory scanning is blocking I/O; keep it off the async worker threads.
    let (root, index) = (
        state.site.static_root.clone(),
        state.site.index_file.clone(),
    );
    let static_entries =
        match tokio::task::spawn_blocking(move || seo::static_html_entries(&root, &index)).await {
            Ok(entries) => entries,
            Err(err) => {
                error!(%err, "static directory scan failed");
                return StatusCode::INTERNAL_SERVER_ERROR.into_response();
            }
        };

    let entries = seo::collect(PAGE_PATHS, &state.blog, static_entries);
    let xml = seo::render_sitemap(&base, &entries);
    (
        [
            (CONTENT_TYPE, "application/xml; charset=utf-8"),
            (CACHE_CONTROL, CACHE),
        ],
        xml,
    )
        .into_response()
}

/// `GET /robots.txt` - crawl rules plus the absolute sitemap URL.
pub async fn robots(State(state): State<AppState>, headers: HeaderMap, uri: Uri) -> Response {
    let body = seo::render_robots(&state.site.base_url(&headers, &uri));
    (
        [
            (CONTENT_TYPE, "text/plain; charset=utf-8"),
            (CACHE_CONTROL, CACHE),
        ],
        body,
    )
        .into_response()
}
