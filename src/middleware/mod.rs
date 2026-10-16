//! Cross-cutting HTTP middleware.
//!
//! Each submodule is independent and configured from [`crate::config::Config`]
//! (except `headers`, whose values are intentionally hard-coded).

pub mod access_log;
pub mod client_ip;
pub mod concurrency;
pub mod headers;
pub mod rate_limit;
pub mod static_files;
pub mod timeout;
