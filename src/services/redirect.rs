//! Redirect target construction and validation.
//!
//! Note: by design this endpoint redirects to caller-supplied destinations
//! (an "open redirect"). Inputs are strictly validated to prevent header
//! injection and URL-confusion tricks (userinfo `@`, odd schemes), but
//! deployments exposing this to browsers should consider a domain allow-list.

use percent_encoding::{utf8_percent_encode, AsciiSet, CONTROLS};

use crate::error::ApiError;

/// Characters escaped inside a single URL path segment.
pub const PATH_SEGMENT: &AsciiSet = &CONTROLS
    .add(b' ')
    .add(b'"')
    .add(b'#')
    .add(b'%')
    .add(b'/')
    .add(b'<')
    .add(b'>')
    .add(b'?')
    .add(b'`')
    .add(b'{')
    .add(b'}');

/// Builds `proto://domain/path[?query]` after validating every component.
///
/// `path` is the already-decoded wildcard capture; it is re-encoded here.
/// `query` is the raw query string, forwarded unchanged.
pub fn build_target(
    proto: &str,
    domain: &str,
    path: &str,
    query: Option<&str>,
) -> Result<String, ApiError> {
    let proto = proto.to_ascii_lowercase();
    if proto != "http" && proto != "https" {
        return Err(ApiError::BadRequest(
            "proto must be 'http' or 'https'".into(),
        ));
    }
    if !is_valid_host(domain) {
        return Err(ApiError::BadRequest("invalid domain".into()));
    }

    let encoded_path = path
        .split('/')
        .map(|segment| utf8_percent_encode(segment, PATH_SEGMENT).to_string())
        .collect::<Vec<_>>()
        .join("/");

    let mut target = format!("{proto}://{domain}/{encoded_path}");
    if let Some(query) = query.filter(|q| !q.is_empty()) {
        target.push('?');
        target.push_str(query);
    }
    Ok(target)
}

/// Accepts `host` or `host:port` where host is a DNS name or IPv4 address.
fn is_valid_host(input: &str) -> bool {
    let (host, port) = match input.rsplit_once(':') {
        Some((h, p)) => (h, Some(p)),
        None => (input, None),
    };
    if let Some(p) = port {
        if p.is_empty()
            || p.len() > 5
            || p.parse::<u16>().is_err()
            || !p.bytes().all(|b| b.is_ascii_digit())
        {
            return false;
        }
    }
    if host.is_empty() || host.len() > 253 {
        return false;
    }
    host.split('.').all(|label| {
        !label.is_empty()
            && label.len() <= 63
            && !label.starts_with('-')
            && !label.ends_with('-')
            && label
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-')
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_url_with_path_and_query() {
        let url = build_target("https", "example.com", "a/b c/d", Some("x=1&y=2")).unwrap();
        assert_eq!(url, "https://example.com/a/b%20c/d?x=1&y=2");
    }

    #[test]
    fn rejects_dangerous_input() {
        assert!(build_target("javascript", "example.com", "x", None).is_err());
        assert!(build_target("https", "evil.com@good.com", "x", None).is_err());
        assert!(build_target("https", "exa mple.com", "x", None).is_err());
        assert!(build_target("https", "example.com:99999", "x", None).is_err());
    }

    #[test]
    fn encodes_injected_control_characters() {
        let url = build_target("http", "example.com", "a\r\nSet-Cookie: x", None).unwrap();
        assert!(!url.contains('\r') && !url.contains('\n'));
    }
}
