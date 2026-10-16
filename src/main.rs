//! Production-grade web server built on Axum 0.8.
//!
//! # Architecture
//!
//! | Module         | Responsibility                                                |
//! |----------------|---------------------------------------------------------------|
//! | [`config`]     | CLI / environment / default configuration                     |
//! | [`logging`]    | `tracing` setup (access log -> stdout, diagnostics -> stderr) |
//! | [`middleware`] | Cross-cutting concerns (logging, limits, headers, static)     |
//! | [`routes`]     | Route tables (API and static files)                           |
//! | [`handlers`]   | Thin HTTP adapters: extract -> call service -> respond        |
//! | [`services`]   | Pure business logic, independent of HTTP                      |
//! | [`app`]        | Assembles the router and the middleware stack                 |
//! | [`shutdown`]   | Signal handling and graceful shutdown with a hard deadline    |

mod app;
mod config;
mod error;
mod handlers;
mod logging;
mod middleware;
mod routes;
mod services;
mod shutdown;
mod state;

use config::Config;
use state::AppState;
use tokio::net::TcpListener;
use tracing::info;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Precedence: CLI > environment (incl. `.env`) > defaults (see `config`).
    let config = Config::load();
    logging::init(config.log_format, config.log_level);

    let state = AppState::new(&config);
    let app = app::build(&config, state.clone());

    let listener = TcpListener::bind(config.socket_addr()).await?;
    info!(
        addr = %listener.local_addr()?,
        static_root = %config.static_root.display(),
        "server listening"
    );

    shutdown::run(listener, app, state.ready.clone(), config.shutdown_timeout()).await?;
    info!("server stopped");
    Ok(())
}
