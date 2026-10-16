//! Shared application state injected into handlers.

use std::{
    sync::{atomic::AtomicBool, Arc},
    time::Instant,
};

use tokio::sync::Semaphore;

use crate::config::Config;

/// Cheaply cloneable state shared across requests.
#[derive(Clone)]
pub struct AppState {
    /// Process start time, used for uptime reporting.
    pub started_at: Instant,
    /// Readiness flag; flipped to `false` once shutdown begins so load
    /// balancers stop routing traffic here.
    pub ready: Arc<AtomicBool>,
    /// Semaphore backing the concurrency limiter (also read by `/health`).
    pub concurrency: Arc<Semaphore>,
    /// Configured size of `concurrency`.
    pub concurrency_limit: usize,
}

impl AppState {
    pub fn new(config: &Config) -> Self {
        Self {
            started_at: Instant::now(),
            ready: Arc::new(AtomicBool::new(true)),
            concurrency: Arc::new(Semaphore::new(config.max_concurrent_requests)),
            concurrency_limit: config.max_concurrent_requests,
        }
    }
}
