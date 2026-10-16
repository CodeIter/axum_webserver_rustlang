//! Graceful shutdown.
//!
//! On SIGINT/SIGTERM the server stops accepting new connections and lets
//! in-flight requests finish. If they do not finish within the configured
//! timeout, [`run`] returns anyway and the process exits.

use std::{
    future::IntoFuture,
    io,
    net::SocketAddr,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};

use axum::Router;
use tokio::{net::TcpListener, signal, sync::Notify};
use tracing::{info, warn};

/// Resolves when Ctrl+C (all platforms) or SIGTERM (Unix) is received.
async fn wait_for_signal() {
    let ctrl_c = async {
        signal::ctrl_c().await.expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = ctrl_c => {},
        () = terminate => {},
    }
}

/// Serves `app` until a termination signal arrives, then drains for at most `timeout`.
pub async fn run(
    listener: TcpListener,
    app: Router,
    ready: Arc<AtomicBool>,
    timeout: Duration,
) -> io::Result<()> {
    // Signals the "drain deadline" branch that shutdown has started.
    let draining = Arc::new(Notify::new());
    let notifier = Arc::clone(&draining);

    let graceful = async move {
        wait_for_signal().await;
        info!(timeout_secs = timeout.as_secs(), "shutdown signal received; draining connections");
        // Fail readiness probes first so orchestrators stop sending traffic.
        ready.store(false, Ordering::SeqCst);
        // `notify_one` stores a permit, so ordering relative to the waiter is safe.
        notifier.notify_one();
    };

    // `into_make_service_with_connect_info` exposes the peer address to middleware.
    let server = axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(graceful)
    .into_future();
    tokio::pin!(server);

    tokio::select! {
        result = &mut server => result,
        () = async {
            draining.notified().await;
            tokio::time::sleep(timeout).await;
        } => {
            warn!("graceful shutdown timed out; forcing termination");
            Ok(())
        }
    }
}
