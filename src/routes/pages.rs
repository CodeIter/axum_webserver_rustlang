//! Template (HTML page) routes.

use axum::{routing::get, Router};

use crate::{handlers::pages, state::AppState};

/// `/` and `/about`, rendered from Handlebars templates.
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(pages::index))
        .route("/about", get(pages::about))
}
