//! Trusted client-IP resolution for deployments behind Render / Cloudflare.
//!
//! Sources are consulted in this order; the first that yields a valid IP wins:
//!
//! 1. `CF-Connecting-IP`  (set by Cloudflare, spoof-resistant)
//! 2. `True-Client-IP`    (set by Cloudflare Enterprise, spoof-resistant)
//! 3. `X-Forwarded-For`   (counted **from the right**, see below)
//! 4. The TCP peer address
//!
//! # X-Forwarded-For
//!
//! Render *appends* to any `X-Forwarded-For` (XFF) chain supplied by the client,
//! so the **leftmost entry is attacker-controlled** and must never be used.
//! Each trusted proxy appends the address it received the connection from, so
//! the real client sits a fixed number of hops from the *right* end:
//!
//! ```text
//! chain: client, cf-edge, render-lb      depth = 3
//!        ^^^^^^ index len - 3
//! ```
//!
//! Anything a client prepends only shifts the left side and is ignored. If the
//! chain is shorter than `depth` it cannot be trusted and we fall back to the socket.

use std::net::{IpAddr, SocketAddr};

use axum::http::HeaderMap;
use serde::Serialize;

/// Where the detected client IP came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum IpSource {
    CfConnectingIp,
    TrueClientIp,
    XForwardedFor,
    Socket,
}

impl IpSource {
    /// Stable, kebab-case name (matches the JSON representation).
    pub fn as_str(self) -> &'static str {
        match self {
            Self::CfConnectingIp => "cf-connecting-ip",
            Self::TrueClientIp => "true-client-ip",
            Self::XForwardedFor => "x-forwarded-for",
            Self::Socket => "socket",
        }
    }
}

/// Detected client address; stored in request extensions by the middleware.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct ClientIp {
    pub ip: IpAddr,
    pub source: IpSource,
}

/// How much of the proxy-supplied information to trust.
#[derive(Debug, Clone, Copy)]
pub struct ClientIpPolicy {
    /// When `false`, headers are ignored and the socket address is used.
    pub trust_proxy_headers: bool,
    /// Number of proxy hops counted from the right of `X-Forwarded-For` (>= 1).
    pub xff_proxy_depth: usize,
}

/// Parses a bare IP, falling back to `ip:port` / `[ip]:port` forms.
fn parse_ip(raw: &str) -> Option<IpAddr> {
    let raw = raw.trim();
    raw.parse::<IpAddr>()
        .ok()
        .or_else(|| raw.parse::<SocketAddr>().ok().map(|a| a.ip()))
        .map(|ip| ip.to_canonical())
}

/// Reads a single-valued header as an IP address.
fn header_ip(headers: &HeaderMap, name: &str) -> Option<IpAddr> {
    parse_ip(headers.get(name)?.to_str().ok()?)
}

/// Picks the client entry `depth` positions from the right of the XFF chain.
///
/// Multiple `X-Forwarded-For` header lines are treated as one chain, in order.
fn xff_ip(headers: &HeaderMap, depth: usize) -> Option<IpAddr> {
    let chain: Vec<&str> = headers
        .get_all("x-forwarded-for")
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(','))
        .map(str::trim)
        .collect();
    // Too-short chain => a proxy is missing; do not guess.
    let index = chain.len().checked_sub(depth)?;
    parse_ip(chain[index])
}

/// Resolves the client IP according to `policy`; see the module docs.
pub fn resolve(headers: &HeaderMap, peer: IpAddr, policy: ClientIpPolicy) -> ClientIp {
    let socket = ClientIp { ip: peer.to_canonical(), source: IpSource::Socket };
    if !policy.trust_proxy_headers {
        return socket;
    }

    if let Some(ip) = header_ip(headers, "cf-connecting-ip") {
        return ClientIp { ip, source: IpSource::CfConnectingIp };
    }
    if let Some(ip) = header_ip(headers, "true-client-ip") {
        return ClientIp { ip, source: IpSource::TrueClientIp };
    }
    if let Some(ip) = xff_ip(headers, policy.xff_proxy_depth.max(1)) {
        return ClientIp { ip, source: IpSource::XForwardedFor };
    }
    socket
}

#[cfg(test)]
mod tests {
    use super::*;

    const POLICY: ClientIpPolicy = ClientIpPolicy { trust_proxy_headers: true, xff_proxy_depth: 3 };
    const PEER: &str = "10.0.0.1";

    fn headers(pairs: &[(&'static str, &str)]) -> HeaderMap {
        let mut map = HeaderMap::new();
        for (k, v) in pairs {
            map.append(*k, v.parse().unwrap());
        }
        map
    }

    fn run(pairs: &[(&'static str, &str)]) -> ClientIp {
        resolve(&headers(pairs), PEER.parse().unwrap(), POLICY)
    }

    #[test]
    fn cf_connecting_ip_has_highest_priority() {
        let r = run(&[
            ("cf-connecting-ip", "203.0.113.5"),
            ("true-client-ip", "203.0.113.6"),
            ("x-forwarded-for", "203.0.113.7, 1.1.1.1, 2.2.2.2"),
        ]);
        assert_eq!((r.ip.to_string().as_str(), r.source), ("203.0.113.5", IpSource::CfConnectingIp));
    }

    #[test]
    fn invalid_cf_header_falls_through_to_true_client_ip() {
        let r = run(&[("cf-connecting-ip", "not-an-ip"), ("true-client-ip", "203.0.113.6")]);
        assert_eq!((r.ip.to_string().as_str(), r.source), ("203.0.113.6", IpSource::TrueClientIp));
    }

    #[test]
    fn xff_counts_from_the_right_and_ignores_spoofed_prefix() {
        // The client sent "6.6.6.6, 7.7.7.7"; Render/Cloudflare appended the rest.
        let r = run(&[("x-forwarded-for", "6.6.6.6, 7.7.7.7, 203.0.113.9, 172.70.0.1, 10.1.1.1")]);
        assert_eq!((r.ip.to_string().as_str(), r.source), ("203.0.113.9", IpSource::XForwardedFor));
    }

    #[test]
    fn xff_spanning_multiple_header_lines() {
        let r = run(&[("x-forwarded-for", "203.0.113.9"), ("x-forwarded-for", "172.70.0.1, 10.1.1.1")]);
        assert_eq!(r.ip.to_string(), "203.0.113.9");
    }

    #[test]
    fn short_xff_chain_falls_back_to_socket() {
        let r = run(&[("x-forwarded-for", "172.70.0.1, 10.1.1.1")]);
        assert_eq!((r.ip.to_string().as_str(), r.source), (PEER, IpSource::Socket));
    }

    #[test]
    fn ignores_headers_when_not_trusted() {
        let policy = ClientIpPolicy { trust_proxy_headers: false, ..POLICY };
        let r = resolve(&headers(&[("cf-connecting-ip", "203.0.113.5")]), PEER.parse().unwrap(), policy);
        assert_eq!(r.source, IpSource::Socket);
    }

    #[test]
    fn supports_ipv6_and_ports() {
        assert_eq!(run(&[("cf-connecting-ip", "2001:db8::1")]).ip.to_string(), "2001:db8::1");
        assert_eq!(run(&[("cf-connecting-ip", "203.0.113.5:4711")]).ip.to_string(), "203.0.113.5");
    }
}
