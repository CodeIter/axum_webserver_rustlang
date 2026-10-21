//! HTTP handlers: thin adapters between Axum extractors and the service layer.

pub mod blog;
pub mod generate;
pub mod kv;
pub mod pages;
pub mod redirect;
pub mod request_info;
pub mod seo;
pub mod system;
