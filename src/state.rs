//! Shared application state injected into handlers.

use std::{
    sync::{atomic::AtomicBool, Arc},
    time::Instant,
};

use tokio::sync::Semaphore;

use crate::{
    config::Config,
    services::{
        blog::Blog,
        site::SiteSettings,
        store::{MemoryStore, SupabaseStore},
        templates::Templates,
    },
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
    /// Public URL and static-directory settings (sitemap / robots.txt).
    pub site: Arc<SiteSettings>,
    /// In-memory key-value store (always available).
    pub memory: Arc<MemoryStore>,
    /// Supabase key-value store; `None` when `SUPABASE_URL`/`SUPABASE_KEY` are not set.
    pub supabase: Option<Arc<SupabaseStore>>,
}

impl AppState {
    pub fn new(
        config: &Config,
        templates: Templates,
        blog: Blog,
        public_url: Option<String>,
        memory: MemoryStore,
        supabase: Option<SupabaseStore>,
    ) -> Self {
        Self {
            started_at: Instant::now(),
            ready: Arc::new(AtomicBool::new(true)),
            concurrency: Arc::new(Semaphore::new(config.max_concurrent_requests)),
            concurrency_limit: config.max_concurrent_requests,
            templates,
            blog: Arc::new(blog),
            memory: Arc::new(memory),
            supabase: supabase.map(Arc::new),
            site: Arc::new(SiteSettings {
                public_url,
                trust_proxy_headers: config.trust_proxy_headers,
                static_root: config.public_dir.clone(),
                index_file: config.public_index_file.clone(),
            }),
        }
    }
}
