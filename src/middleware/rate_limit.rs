//! Per-client fixed-window rate limiter.
//!
//! Hardening choices:
//! * Clients are keyed by the **trusted client IP** resolved by
//!   [`crate::middleware::client_ip`] (never the raw, spoofable leftmost
//!   `X-Forwarded-For` entry).
//! * IPv4-mapped IPv6 addresses are canonicalised and IPv6 clients are keyed by
//!   their `/64` prefix, so rotating addresses inside one subnet doesn't evade limits.
//! * Memory is bounded: expired windows are purged periodically and the table
//!   has a hard cap (fails closed when exceeded).

use std::{
    collections::HashMap,
    net::{IpAddr, Ipv6Addr},
    sync::{Arc, Mutex, PoisonError},
    time::{Duration, Instant},
};

use axum::{
    extract::{Request, State},
    http::{header::RETRY_AFTER, HeaderValue, StatusCode},
    middleware::Next,
    response::Response,
    Extension,
};

use crate::{error::json_error, services::client_ip::ClientIp};

/// Upper bound on tracked clients, protecting memory under address-spray attacks.
const MAX_TRACKED_CLIENTS: usize = 100_000;

/// Request counter for one client within the current window.
struct Window {
    started: Instant,
    count: u32,
}

/// Thread-safe fixed-window limiter.
pub struct RateLimiter {
    max_requests: u32,
    window: Duration,
    clients: Mutex<HashMap<IpAddr, Window>>,
}

impl RateLimiter {
    pub fn new(max_requests: u32, window: Duration) -> Self {
        Self {
            max_requests,
            window,
            clients: Mutex::new(HashMap::new()),
        }
    }

    /// Normalises an address into a rate-limit key.
    fn key(ip: IpAddr) -> IpAddr {
        match ip.to_canonical() {
            IpAddr::V6(v6) => {
                let s = v6.segments();
                IpAddr::V6(Ipv6Addr::new(s[0], s[1], s[2], s[3], 0, 0, 0, 0))
            }
            v4 => v4,
        }
    }

    /// Records a request. `Err(retry_after)` means the client is over the limit.
    pub fn check(&self, ip: IpAddr) -> Result<(), Duration> {
        let now = Instant::now();
        let key = Self::key(ip);
        // The critical section cannot panic mid-update, so recover from poisoning.
        let mut clients = self.clients.lock().unwrap_or_else(PoisonError::into_inner);

        if clients.len() >= MAX_TRACKED_CLIENTS && !clients.contains_key(&key) {
            Self::purge(&mut clients, now, self.window);
            if clients.len() >= MAX_TRACKED_CLIENTS {
                return Err(self.window);
            }
        }

        let entry = clients.entry(key).or_insert(Window {
            started: now,
            count: 0,
        });
        if now.duration_since(entry.started) >= self.window {
            *entry = Window {
                started: now,
                count: 0,
            };
        }
        if entry.count >= self.max_requests {
            return Err(self
                .window
                .saturating_sub(now.duration_since(entry.started)));
        }
        entry.count += 1;
        Ok(())
    }

    fn purge(clients: &mut HashMap<IpAddr, Window>, now: Instant, window: Duration) {
        clients.retain(|_, w| now.duration_since(w.started) < window);
    }

    /// Spawns a background task that periodically drops expired windows.
    pub fn spawn_janitor(self: &Arc<Self>) {
        let limiter = Arc::clone(self);
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(limiter.window);
            loop {
                ticker.tick().await;
                let mut clients = limiter
                    .clients
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner);
                Self::purge(&mut clients, Instant::now(), limiter.window);
            }
        });
    }
}

/// Rejects over-limit clients with `429 Too Many Requests` and `Retry-After`.
pub async fn rate_limit(
    State(limiter): State<Arc<RateLimiter>>,
    Extension(client): Extension<ClientIp>,
    request: Request,
    next: Next,
) -> Response {
    match limiter.check(client.ip) {
        Ok(()) => next.run(request).await,
        Err(retry_after) => {
            let mut response = json_error(StatusCode::TOO_MANY_REQUESTS, "rate limit exceeded");
            response
                .headers_mut()
                .insert(RETRY_AFTER, HeaderValue::from(retry_after.as_secs().max(1)));
            response
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks_after_threshold_and_isolates_clients() {
        let limiter = RateLimiter::new(2, Duration::from_secs(60));
        let a: IpAddr = "192.0.2.1".parse().unwrap();
        let b: IpAddr = "192.0.2.2".parse().unwrap();
        assert!(limiter.check(a).is_ok());
        assert!(limiter.check(a).is_ok());
        assert!(limiter.check(a).is_err());
        assert!(limiter.check(b).is_ok());
    }

    #[test]
    fn ipv6_clients_share_a_slash_64() {
        let limiter = RateLimiter::new(1, Duration::from_secs(60));
        assert!(limiter.check("2001:db8::1".parse().unwrap()).is_ok());
        assert!(limiter.check("2001:db8::2".parse().unwrap()).is_err());
    }
}
