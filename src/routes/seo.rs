//! SEO routes: `/sitemap.xml` and `/robots.txt`.

use axum::{routing::get, Router};

use crate::{handlers::seo, state::AppState};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/sitemap.xml", get(seo::sitemap))
        .route("/robots.txt", get(seo::robots))
}
