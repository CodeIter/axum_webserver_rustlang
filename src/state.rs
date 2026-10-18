//! Shared application state injected into handlers.

use std::{
    sync::{atomic::AtomicBool, Arc},
    time::Instant,
};

use tokio::sync::Semaphore;

use crate::{
    config::Config,
    services::{blog::Blog, templates::Templates},
};

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
    /// Loaded Handlebars templates.
    pub templates: Templates,
    /// Published blog posts, loaded at startup.
    pub blog: Arc<Blog>,
}

impl AppState {
    pub fn new(config: &Config, templates: Templates, blog: Blog) -> Self {
        Self {
            started_at: Instant::now(),
            ready: Arc::new(AtomicBool::new(true)),
            concurrency: Arc::new(Semaphore::new(config.max_concurrent_requests)),
            concurrency_limit: config.max_concurrent_requests,
            templates,
            blog: Arc::new(blog),
        }
    }
}
