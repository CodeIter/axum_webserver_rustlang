//! Logging setup.
//!
//! * The **access log** (target [`ACCESS_LOG_TARGET`]) goes to **stdout**.
//! * Every other event (startup, warnings, errors, ...) goes to **stderr**,
//!   filtered by the configured level.
//!
//! Both streams honour the text/JSON switch.

use std::io::{self, IsTerminal};

use tracing::Metadata;
use tracing_subscriber::{
    filter::filter_fn, fmt, fmt::MakeWriter, layer::SubscriberExt, util::SubscriberInitExt, Layer,
    Registry,
};

use crate::config::{LogFormat, LogLevel};

/// `tracing` target used exclusively by the request logger.
pub const ACCESS_LOG_TARGET: &str = "access_log";

type BoxedLayer = Box<dyn Layer<Registry> + Send + Sync + 'static>;

/// Builds a formatting layer for the given format and writer.
fn fmt_layer<W>(format: LogFormat, writer: W, ansi: bool) -> BoxedLayer
where
    W: for<'w> MakeWriter<'w> + Send + Sync + 'static,
{
    match format {
        LogFormat::Text => fmt::layer().with_writer(writer).with_ansi(ansi).boxed(),
        LogFormat::Json => fmt::layer().json().with_writer(writer).boxed(),
    }
}

/// Installs the global subscriber. Call once, early in `main`.
pub fn init(format: LogFormat, level: LogLevel) {
    let max_level = level.as_tracing();

    let access = fmt_layer(format, io::stdout, io::stdout().is_terminal())
        .with_filter(filter_fn(|meta: &Metadata<'_>| {
            meta.target() == ACCESS_LOG_TARGET
        }))
        .boxed();

    // `Level` ordering: more verbose == greater, so `<=` keeps this level and below.
    let diagnostics = fmt_layer(format, io::stderr, io::stderr().is_terminal())
        .with_filter(filter_fn(move |meta: &Metadata<'_>| {
            meta.target() != ACCESS_LOG_TARGET && *meta.level() <= max_level
        }))
        .boxed();

    tracing_subscriber::registry()
        .with(vec![access, diagnostics])
        .init();
}
