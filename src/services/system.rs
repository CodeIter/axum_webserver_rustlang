//! System and health metrics.
//!
//! Process memory and load average are read from `/proc` (Linux); on other
//! platforms those fields are reported as `null`.

use std::{fs, time::Duration};

use serde::Serialize;

/// Full health report returned by `/api/v1/health`.
#[derive(Debug, Serialize)]
pub struct HealthReport {
    pub status: &'static str,
    pub version: &'static str,
    pub uptime_seconds: u64,
    pub system: SystemMetrics,
    pub concurrency: ConcurrencyMetrics,
}

#[derive(Debug, Serialize)]
pub struct SystemMetrics {
    pub pid: u32,
    pub cpu_cores: usize,
    /// Resident set size of this process in KiB.
    pub memory_rss_kib: Option<u64>,
    /// 1, 5 and 15 minute load averages.
    pub load_average: Option<[f64; 3]>,
}

#[derive(Debug, Serialize)]
pub struct ConcurrencyMetrics {
    pub limit: usize,
    /// Requests currently being processed (includes this health check).
    pub in_flight: usize,
}

/// Collects a health report from the supplied runtime figures.
pub fn collect(
    uptime: Duration,
    concurrency_limit: usize,
    available_permits: usize,
) -> HealthReport {
    HealthReport {
        status: "ok",
        version: env!("CARGO_PKG_VERSION"),
        uptime_seconds: uptime.as_secs(),
        system: SystemMetrics {
            pid: std::process::id(),
            cpu_cores: std::thread::available_parallelism().map_or(1, usize::from),
            memory_rss_kib: read_rss_kib(),
            load_average: read_load_average(),
        },
        concurrency: ConcurrencyMetrics {
            limit: concurrency_limit,
            in_flight: concurrency_limit.saturating_sub(available_permits),
        },
    }
}

/// Parses `VmRSS:   12345 kB` from `/proc/self/status` (procfs reads are in-memory and fast).
fn read_rss_kib() -> Option<u64> {
    fs::read_to_string("/proc/self/status")
        .ok()?
        .lines()
        .find_map(|line| line.strip_prefix("VmRSS:"))
        .and_then(|rest| rest.split_whitespace().next()?.parse().ok())
}

/// Parses the first three fields of `/proc/loadavg`.
fn read_load_average() -> Option<[f64; 3]> {
    let content = fs::read_to_string("/proc/loadavg").ok()?;
    let mut fields = content.split_whitespace().map(|f| f.parse::<f64>());
    Some([
        fields.next()?.ok()?,
        fields.next()?.ok()?,
        fields.next()?.ok()?,
    ])
}
