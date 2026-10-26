//! Centralized configuration.
//!
//! Every tunable lives in [`Config`]. Values are resolved with this strict
//! precedence (highest first):
//!
//! 1. Command-line arguments
//! 2. Environment variables, including those loaded from a `.env` file
//!    (real environment variables win over `.env` entries)
//! 3. The defaults in the [`defaults`] module below
//!
//! `clap` natively implements the CLI > env > default ordering; `.env` is
//! loaded into the process environment before parsing.

use std::{
    convert::Infallible,
    fmt,
    net::{IpAddr, SocketAddr},
    path::PathBuf,
    str::FromStr,
    time::Duration,
};

use clap::{error::ErrorKind, ArgAction, Parser, ValueEnum};

/// Built-in default values (lowest precedence).
pub mod defaults {

    pub const HOST: &str = "0.0.0.0";
    pub const PORT: u16 = 8000;

    pub const REQUEST_TIMEOUT_SECS: u64 = 30;
    pub const SHUTDOWN_TIMEOUT_SECS: u64 = 30;

    pub const TEMPLATES_DIR: &str = "./templates";
    pub const BLOG_DIR: &str = "./blog";
    pub const PUBLIC_DIR: &str = "./public";
    pub const STATIC_DIR: &str = "./static";
    pub const RESOURCES_DIR: &str = "./resources";

    pub const PUBLIC_INDEX_FILE: &str = "index.html";

    pub const RATE_LIMIT_MAX_REQUESTS: u32 = 100;
    pub const RATE_LIMIT_WINDOW_SECS: u64 = 60;

    pub const MAX_CONCURRENT_REQUESTS: usize = 1024;

    pub const SUPABASE_TABLE: &str = "kv_store";
    pub const SUPABASE_TIMEOUT_SECS: u64 = 10;

    pub const MEMORY_STORE_MAX_ENTRIES: usize = 10_000;

    pub const TRUST_PROXY_HEADERS: bool = false;
    pub const XFF_PROXY_DEPTH: usize = 1;
}

/// A secret string; its `Debug` output is redacted so it cannot leak into logs.
#[derive(Clone)]
pub struct Secret(String);

impl Secret {
    /// The secret value. Handle with care.
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Secret(***)")
    }
}

impl FromStr for Secret {
    type Err = Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(s.to_owned()))
    }
}

/// What to do when a request targets a dotfile (e.g. `/.env`, `/.git/config`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum DotfilePolicy {
    /// Serve dotfiles like any other file (not recommended).
    Allow,
    /// Respond `403 Forbidden`.
    Deny,
    /// Respond `404 Not Found`, hiding the file's existence.
    Ignore,
}

/// Output format for all log lines.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum LogFormat {
    /// Human-readable text.
    Text,
    /// One JSON object per line.
    Json,
}

/// Verbosity of diagnostic (stderr) logs. The access log is always emitted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum LogLevel {
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}

impl LogLevel {
    /// Converts to the equivalent [`tracing::Level`].
    pub fn as_tracing(self) -> tracing::Level {
        match self {
            Self::Error => tracing::Level::ERROR,
            Self::Warn => tracing::Level::WARN,
            Self::Info => tracing::Level::INFO,
            Self::Debug => tracing::Level::DEBUG,
            Self::Trace => tracing::Level::TRACE,
        }
    }
}

/// Complete server configuration.
#[derive(Debug, Clone, Parser)]
#[command(
    name = "axum-server",
    version,
    about = "Production-grade Axum web server"
)]
pub struct Config {
    /// IP address to bind to.
    #[arg(long, env = "SERVER_HOST", default_value = defaults::HOST)]
    pub host: IpAddr,

    /// TCP port to listen on.
    #[arg(long, env = "PORT", default_value_t = defaults::PORT)]
    pub port: u16,

    /// Log output format (applies to access log and diagnostics).
    #[arg(long, env = "LOG_FORMAT", value_enum, default_value_t = LogFormat::Text)]
    pub log_format: LogFormat,

    /// Diagnostic log verbosity (stderr).
    #[arg(long, env = "LOG_LEVEL", value_enum, default_value_t = LogLevel::Info)]
    pub log_level: LogLevel,

    /// Seconds to wait for in-flight requests on shutdown before forcing exit.
    #[arg(long, env = "SHUTDOWN_TIMEOUT_SECS", default_value_t = defaults::SHUTDOWN_TIMEOUT_SECS)]
    pub shutdown_timeout_secs: u64,

    /// Maximum time (seconds) a request may take before `408` is returned.
    #[arg(long, env = "REQUEST_TIMEOUT_SECS", default_value_t = defaults::REQUEST_TIMEOUT_SECS)]
    pub request_timeout_secs: u64,

    /// Directory containing Handlebars templates (layouts/, partials/, pages).
    #[arg(long, env = "TEMPLATES_DIR", default_value = defaults::TEMPLATES_DIR)]
    pub templates_dir: PathBuf,

    /// Directory containing Markdown blog posts (`*.md` with YAML front-matter).
    #[arg(long, env = "BLOG_DIR", default_value = defaults::BLOG_DIR)]
    pub blog_dir: PathBuf,

    /// Directory served at `/`.
    #[arg(long, env = "PUBLIC_DIR", default_value = defaults::PUBLIC_DIR)]
    pub public_dir: PathBuf,

    /// Directory served at `/static`.
    #[arg(long, env = "STATIC_DIR", default_value = defaults::STATIC_DIR)]
    pub static_dir: PathBuf,

    /// Directory served at `/resources`.
    #[arg(long, env = "RESOURCES_DIR", default_value = defaults::RESOURCES_DIR)]
    pub resources_dir: PathBuf,

    /// File served for directory requests.
    #[arg(long, env = "PUBLIC_INDEX_FILE", default_value = defaults::PUBLIC_INDEX_FILE)]
    pub public_index_file: String,

    /// Policy for requests that touch dotfiles.
    #[arg(long, env = "PUBLIC_DOTFILES", value_enum, default_value_t = DotfilePolicy::Ignore)]
    pub public_dotfiles: DotfilePolicy,

    /// Maximum requests per client IP per window.
    #[arg(long, env = "RATE_LIMIT_MAX_REQUESTS", default_value_t = defaults::RATE_LIMIT_MAX_REQUESTS)]
    pub rate_limit_max_requests: u32,

    /// Rate-limit window length in seconds.
    #[arg(long, env = "RATE_LIMIT_WINDOW_SECS", default_value_t = defaults::RATE_LIMIT_WINDOW_SECS)]
    pub rate_limit_window_secs: u64,

    /// Maximum number of requests processed simultaneously.
    #[arg(long, env = "MAX_CONCURRENT_REQUESTS", default_value_t = defaults::MAX_CONCURRENT_REQUESTS)]
    pub max_concurrent_requests: usize,

    /// Public base URL (e.g. `https://example.com`) used in `sitemap.xml` and
    /// `robots.txt`. Overrides `RENDER_EXTERNAL_URL`, `RENDER_EXTERNAL_HOSTNAME`,
    /// `EXTERNAL_URL` and `EXTERNAL_HOSTNAME`, which are consulted in that order.
    #[arg(long)]
    pub public_url: Option<String>,

    /// Trust `CF-Connecting-IP`, `True-Client-IP` and `X-Forwarded-For` for the
    /// client IP. Disable when the server is reachable without a trusted proxy.
    #[arg(long, env = "TRUST_PROXY_HEADERS", action = ArgAction::Set, default_value_t = defaults::TRUST_PROXY_HEADERS)]
    pub trust_proxy_headers: bool,

    /// Proxy hops counted from the *right* of `X-Forwarded-For` to find the
    /// client (Render: client, cloudflare edge, render lb => 3).
    #[arg(long, env = "XFF_PROXY_DEPTH", default_value_t = defaults::XFF_PROXY_DEPTH)]
    pub xff_proxy_depth: usize,

    /// Maximum number of entries in the in-memory store.
    #[arg(long, env = "MEMORY_STORE_MAX_ENTRIES", default_value_t = defaults::MEMORY_STORE_MAX_ENTRIES)]
    pub memory_store_max_entries: usize,

    /// Supabase project URL (e.g. `https://<ref>.supabase.co`). Together with
    /// `--supabase-key` it enables the Supabase store.
    #[arg(long, env = "SUPABASE_URL")]
    pub supabase_url: Option<String>,

    /// Supabase API key. Use the server-side `service_role` key and prefer the
    /// environment variable over the flag (flags are visible in process lists).
    #[arg(long, env = "SUPABASE_KEY", hide_env_values = true)]
    pub supabase_key: Option<Secret>,

    /// Table used by the Supabase store (see `supabase/kv_store.sql`).
    #[arg(long, env = "SUPABASE_TABLE", default_value = defaults::SUPABASE_TABLE)]
    pub supabase_table: String,

    /// Timeout (seconds) for Supabase requests.
    #[arg(long, env = "SUPABASE_TIMEOUT_SECS", default_value_t = defaults::SUPABASE_TIMEOUT_SECS)]
    pub supabase_timeout_secs: u64,
}

impl Config {
    /// Loads `.env` (if present), parses CLI/env, and validates the result.
    ///
    /// Exits the process with a usage error on invalid input.
    pub fn load() -> Self {
        // A missing `.env` is fine; existing real env vars are never overridden.
        let _ = dotenvy::dotenv();
        let config = Self::parse();
        if let Err(message) = config.validate() {
            clap::Error::raw(ErrorKind::ValueValidation, format!("{message}\n")).exit();
        }
        config
    }

    /// Checks range constraints that clap cannot express.
    fn validate(&self) -> Result<(), String> {
        if self.request_timeout_secs == 0 {
            return Err("request timeout must be greater than zero".into());
        }
        if self.rate_limit_max_requests == 0 || self.rate_limit_window_secs == 0 {
            return Err("rate limit threshold and window must be greater than zero".into());
        }
        if self.max_concurrent_requests == 0 {
            return Err("max concurrent requests must be greater than zero".into());
        }
        let has_url = self
            .supabase_url
            .as_deref()
            .is_some_and(|v| !v.trim().is_empty());
        let has_key = self
            .supabase_key
            .as_ref()
            .is_some_and(|k| !k.expose().trim().is_empty());
        if has_url != has_key {
            return Err("SUPABASE_URL and SUPABASE_KEY must be set together".into());
        }
        if self.supabase_timeout_secs == 0 || self.memory_store_max_entries == 0 {
            return Err("supabase timeout and memory store size must be greater than zero".into());
        }
        if let Some(url) = &self.public_url {
            if crate::services::site::normalize_base_url(url).is_none() {
                return Err(format!("public URL {url:?} is not a valid URL or hostname"));
            }
        }
        if self.xff_proxy_depth == 0 {
            return Err("xff proxy depth must be at least 1".into());
        }
        if self.public_index_file.is_empty() || self.public_index_file.contains(['/', '\\']) {
            return Err("static index file must be a plain, non-empty file name".into());
        }
        Ok(())
    }

    /// Address the listener binds to.
    pub fn socket_addr(&self) -> SocketAddr {
        SocketAddr::new(self.host, self.port)
    }

    /// `(url, key)` when the Supabase store is configured.
    pub fn supabase_credentials(&self) -> Option<(&str, &str)> {
        let url = self.supabase_url.as_deref()?.trim();
        let key = self.supabase_key.as_ref()?.expose().trim();
        (!url.is_empty() && !key.is_empty()).then_some((url, key))
    }

    pub fn supabase_timeout(&self) -> Duration {
        Duration::from_secs(self.supabase_timeout_secs)
    }

    pub fn request_timeout(&self) -> Duration {
        Duration::from_secs(self.request_timeout_secs)
    }

    pub fn rate_limit_window(&self) -> Duration {
        Duration::from_secs(self.rate_limit_window_secs)
    }

    pub fn shutdown_timeout(&self) -> Duration {
        Duration::from_secs(self.shutdown_timeout_secs)
    }
}
