//! HTML page handlers (Handlebars templates).

use axum::{
    extract::State,
    http::StatusCode,
    response::{Html, IntoResponse, Response},
};
use serde_json::{json, Value};
use tracing::error;

use crate::{handlers::blog, services::templates::Templates, state::AppState};

/// Number of posts shown on the home page.
const HOME_PAGE_SIZE: usize = 5;

/// Renders `page` with `data`, mapping failures to a bare `500`.
pub fn render(templates: &Templates, status: StatusCode, page: &str, data: Value) -> Response {
    match templates.render_page(page, data) {
        Ok(html) => (status, Html(html)).into_response(),
        Err(err) => {
            error!(page, %err, "template rendering failed");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

/// `GET /`
///
/// Shows the latest [`HOME_PAGE_SIZE`] posts; "Load more" fetches the rest.
pub async fn index(State(state): State<AppState>) -> Response {
    let listing = state.blog.listing(0, HOME_PAGE_SIZE);
    let data = blog::listing_context("Home", "/", &listing);
    render(&state.templates, StatusCode::OK, "index", data)
}

/// `GET /about`
pub async fn about(State(state): State<AppState>) -> Response {
    render(&state.templates, StatusCode::OK, "about", json!({ "title": "About", "current_path": "/about" }))
}

/// The custom `404 Not Found` page; shared by every "not found" path.
pub fn not_found_page(templates: &Templates) -> Response {
    render(
        templates,
        StatusCode::NOT_FOUND,
        "404",
        json!({ "title": "Page not found", "current_path": "" }),
    )
}

/// Handler form of [`not_found_page`], used as the static service's fallback.
pub async fn not_found(State(templates): State<Templates>) -> Response {
    not_found_page(&templates)
}
