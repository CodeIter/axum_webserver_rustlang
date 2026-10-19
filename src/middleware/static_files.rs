//! Guard placed in front of the static file service.
//!
//! Responsibilities:
//! 1. Enforce the dotfile policy (checked on the *percent-decoded* path so
//!    `%2e%65nv` cannot bypass it).
//! 2. Serve the configurable index file for directory requests, by rewriting
//!    `/dir/` to `/dir/<index>`; `/dir` redirects to `/dir/`.

use std::{path::PathBuf, sync::Arc};

use axum::{
    extract::{Request, State},
    http::{uri::PathAndQuery, StatusCode, Uri},
    middleware::Next,
    response::{IntoResponse, Redirect, Response},
};
use percent_encoding::{percent_decode_str, utf8_percent_encode};

use crate::{
    config::DotfilePolicy,
    handlers::pages,
    services::{redirect::PATH_SEGMENT, templates::Templates},
};

/// Immutable settings for [`static_guard`].
pub struct StaticGuard {
    /// Used to render the custom 404 page for ignored dotfiles.
    pub templates: Templates,
    pub root: PathBuf,
    pub index_file: String,
    pub dotfiles: DotfilePolicy,
}

/// Returns `true` if any path segment starts with a dot (includes `.` and `..`).
fn touches_dotfile(decoded_path: &str) -> bool {
    decoded_path
        .split('/')
        .any(|segment| segment.starts_with('.'))
}

/// Replaces the request path, preserving the query string.
fn rewrite_path(request: &mut Request, new_path: &str) {
    let path_and_query = match request.uri().query() {
        Some(query) => format!("{new_path}?{query}"),
        None => new_path.to_owned(),
    };
    let mut parts = request.uri().clone().into_parts();
    if let Ok(pq) = path_and_query.parse::<PathAndQuery>() {
        parts.path_and_query = Some(pq);
        if let Ok(uri) = Uri::from_parts(parts) {
            *request.uri_mut() = uri;
        }
    }
}

/// Middleware entry point; see the module docs.
pub async fn static_guard(
    State(guard): State<Arc<StaticGuard>>,
    mut request: Request,
    next: Next,
) -> Response {
    let raw_path = request.uri().path().to_owned();
    let decoded = percent_decode_str(&raw_path)
        .decode_utf8_lossy()
        .into_owned();

    if guard.dotfiles != DotfilePolicy::Allow && touches_dotfile(&decoded) {
        return match guard.dotfiles {
            DotfilePolicy::Deny => StatusCode::FORBIDDEN.into_response(),
            // `Ignore`: indistinguishable from any other missing file.
            _ => pages::not_found_page(&guard.templates),
        };
    }

    if raw_path.ends_with('/') {
        let index = utf8_percent_encode(&guard.index_file, PATH_SEGMENT);
        rewrite_path(&mut request, &format!("{raw_path}{index}"));
    } else if !decoded.split('/').any(|s| s == "..") {
        // Directory without trailing slash: redirect so relative links resolve.
        let candidate = guard.root.join(decoded.trim_start_matches('/'));
        let is_dir = tokio::fs::metadata(candidate)
            .await
            .map(|m| m.is_dir())
            .unwrap_or(false);
        if is_dir {
            let target = match request.uri().query() {
                Some(q) => format!("{raw_path}/?{q}"),
                None => format!("{raw_path}/"),
            };
            return Redirect::permanent(&target).into_response();
        }
    }

    next.run(request).await
}
