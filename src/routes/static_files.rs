//! Static file service mounted at `/`.

use std::sync::Arc;

use axum::{middleware::from_fn_with_state, Router};
use tower_http::services::ServeDir;

use crate::{
    config::Config,
    middleware::static_files::{static_guard, StaticGuard},
};

/// Serves `config.static_root`, guarded by the dotfile policy and index handling.
pub fn router(config: &Config) -> Router {
    // Index handling is done by `static_guard` so the file name is configurable.
    let serve_dir = ServeDir::new(&config.static_root).append_index_html_on_directories(false);

    let guard = Arc::new(StaticGuard {
        root: config.static_root.clone(),
        index_file: config.static_index_file.clone(),
        dotfiles: config.static_dotfiles,
    });

    Router::new()
        .fallback_service(serve_dir)
        .layer(from_fn_with_state(guard, static_guard))
}
