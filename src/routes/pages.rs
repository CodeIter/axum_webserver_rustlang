//! Template (HTML page) routes.

use axum::{routing::get, Router};

use crate::{
    handlers::{blog, pages},
    state::AppState,
};

/// Paths of the template pages without parameters. These are listed in
/// `sitemap.xml`, so add new static pages here *and* in [`router`].
pub const PAGE_PATHS: &[&str] = &["/", "/about", "/hello/world", "/blog"];

/// `/`, `/about`, `/hello/world`, `/blog` and `/blog/{date}/{slug}`, rendered from Handlebars templates.
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(pages::index))
        .route("/about", get(pages::about))
        .route("/hello/world", get(pages::hello_world))
        .route("/blog", get(blog::index))
        .route("/blog/{date}/{slug}", get(blog::post))
}
