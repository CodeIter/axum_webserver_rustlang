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
use services::{
    blog::Blog,
    site,
    store::{MemoryStore, SupabaseStore},
    templates::Templates,
};
use state::AppState;
use tokio::net::TcpListener;
use tracing::{error, info, warn};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Precedence: CLI > environment (incl. `.env`) > defaults (see `config`).
    let config = Config::load();
    logging::init(config.log_format, config.log_level);

    // Fail fast: a server without its templates cannot render pages or 404s.
    let templates = Templates::load(&config.templates_dir).unwrap_or_else(|err| {
        error!(%err, "cannot start");
        std::process::exit(1);
    });

    // A missing blog directory only logs a warning: the site still works.
    let blog = Blog::load(&config.blog_dir);

    // Public hostname: --public-url > RENDER_EXTERNAL_* > EXTERNAL_*.
    let resolution = site::resolve_public_url(config.public_url.as_deref(), |name| {
        std::env::var(name).ok()
    });
    for message in &resolution.warnings {
        warn!("{message}");
    }
    match (&resolution.url, resolution.source) {
        (Some(url), Some(source)) => info!(public_url = %url, source, "public URL resolved"),
        _ => warn!(
            "no public URL configured (set RENDER_EXTERNAL_URL / RENDER_EXTERNAL_HOSTNAME / \
             EXTERNAL_URL / EXTERNAL_HOSTNAME or --public-url); sitemap.xml and robots.txt \
             fall back to the request Host header"
        ),
    }

    // Key-value stores: in-memory always; Supabase only when credentials are configured.
    let memory = MemoryStore::new(config.memory_store_max_entries);
    let supabase = config.supabase_credentials().map(|(url, key)| {
        SupabaseStore::new(url, key, &config.supabase_table, config.supabase_timeout())
            .unwrap_or_else(|err| {
                error!(%err, "invalid Supabase configuration");
                std::process::exit(1);
            })
    });

    let state = AppState::new(&config, templates, blog, resolution.url, memory, supabase);

    // Probe Supabase in the background so startup never waits on (or fails because of) it.
    match &state.supabase {
        Some(store) => {
            let store = std::sync::Arc::clone(store);
            tokio::spawn(async move {
                match store.health_check().await {
                    Ok(health) => {
                        info!(latency_ms = health.latency_ms, table = %health.table, "Supabase is reachable")
                    }
                    Err(err) => warn!(%err, "Supabase health probe failed"),
                }
            });
        }
        None => info!("Supabase store disabled (SUPABASE_URL / SUPABASE_KEY not set)"),
    }
    let app = app::build(&config, state.clone());

    let listener = TcpListener::bind(config.socket_addr()).await?;
    info!(
        addr = %listener.local_addr()?,
        public_dir = %&config.public_dir.display(),
        templates_dir = %config.templates_dir.display(),
        blog_dir = %config.blog_dir.display(),
        "server listening"
    );

    shutdown::run(
        listener,
        app,
        state.ready.clone(),
        config.shutdown_timeout(),
    )
    .await?;
    info!("server stopped");
    Ok(())
}
