//! Static file service mounted at `/`.

use std::sync::Arc;

use axum::{handler::Handler, middleware::from_fn_with_state, Router};
use tower_http::services::ServeDir;

use crate::{
    config::Config,
    handlers::pages,
    middleware::static_files::{static_guard, StaticGuard},
    services::templates::Templates,
};

/// Serves `config.public_dir`, guarded by the dotfile policy and index handling.
///
/// This is the last stop of the request pipeline: files that do not exist get
/// the custom 404 page (status 404) rendered from `404.hbs`.
pub fn router(config: &Config, templates: Templates) -> Router {
    // Index handling is done by `static_guard` so the file name is configurable.
    let serve_public_dir = ServeDir::new(&config.public_dir)
        .append_index_html_on_directories(false)
        .not_found_service(pages::not_found.with_state(templates.clone()));
    let serve_static_dir = ServeDir::new(&config.static_dir)
        .append_index_html_on_directories(false)
        .not_found_service(pages::not_found.with_state(templates.clone()));
    let serve_resources_dir = ServeDir::new(&config.resources_dir)
        .append_index_html_on_directories(false)
        .not_found_service(pages::not_found.with_state(templates.clone()));

    let guard = Arc::new(StaticGuard {
        root: config.public_dir.clone(),
        index_file: config.public_index_file.clone(),
        dotfiles: config.public_dotfiles,
        templates,
    });

    Router::new()
        .nest_service("/static", serve_static_dir)
        .nest_service("/resources", serve_resources_dir)
        .fallback_service(serve_public_dir)
        .layer(from_fn_with_state(guard, static_guard))
}
