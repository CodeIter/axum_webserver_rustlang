//! Template (HTML page) routes.

use axum::{routing::get, Router};

use crate::{
    handlers::{blog, pages},
    state::AppState,
};

/// `/`, `/about`, `/hello/world`, `/blog` and `/blog/{date}/{slug}`, rendered from Handlebars templates.
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(pages::index))
        .route("/about", get(pages::about))
        .route("/hello/world", get(pages::hello_world))
        .route("/blog", get(blog::index))
        .route("/blog/{date}/{slug}", get(blog::post))
}
