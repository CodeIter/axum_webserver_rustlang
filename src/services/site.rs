//! Public site identity: which absolute URL the application is reachable at.
//!
//! Resolution order (first valid, non-empty value wins):
//!
//! 1. `--public-url` command-line flag
//! 2. `RENDER_EXTERNAL_URL`, then `RENDER_EXTERNAL_HOSTNAME`  (set by Render)
//! 3. `EXTERNAL_URL`, then `EXTERNAL_HOSTNAME`                (generic fallback)
//!
//! Values may be a full URL (`https://myapp.onrender.com`) or a bare hostname
//! (`myapp.onrender.com`, assumed `https`). Only the origin is kept: any path,
//! query or fragment is dropped.

use std::path::PathBuf;

use axum::http::{header::HOST, HeaderMap, Uri};

/// Environment variables consulted, highest priority first.
pub const ENV_PRIORITY: [&str; 4] = [
    "RENDER_EXTERNAL_URL",
    "RENDER_EXTERNAL_HOSTNAME",
    "EXTERNAL_URL",
    "EXTERNAL_HOSTNAME",
];

/// Outcome of [`resolve_public_url`].
#[derive(Debug, PartialEq, Eq)]
pub struct Resolution {
    /// Normalised origin such as `https://myapp.onrender.com`.
    pub url: Option<String>,
    /// Where `url` came from (flag or environment variable name).
    pub source: Option<&'static str>,
    /// Variables that were set but unusable.
    pub warnings: Vec<String>,
}

/// Determines the public base URL; `get_env` abstracts `std::env::var` for tests.
pub fn resolve_public_url(
    cli: Option<&str>,
    get_env: impl Fn(&str) -> Option<String>,
) -> Resolution {
    let mut warnings = Vec::new();

    if let Some(raw) = cli {
        if let Some(url) = normalize_base_url(raw) {
            return Resolution {
                url: Some(url),
                source: Some("--public-url"),
                warnings,
            };
        }
        warnings.push(format!(
            "--public-url {raw:?} is not a valid URL or hostname; ignoring it"
        ));
    }

    for name in ENV_PRIORITY {
        let Some(raw) = get_env(name)
            .map(|v| v.trim().to_owned())
            .filter(|v| !v.is_empty())
        else {
            continue;
        };
        match normalize_base_url(&raw) {
            Some(url) => {
                return Resolution {
                    url: Some(url),
                    source: Some(name),
                    warnings,
                }
            }
            None => warnings.push(format!(
                "{name} is set but is not a valid URL or hostname; ignoring it"
            )),
        }
    }
    Resolution {
        url: None,
        source: None,
        warnings,
    }
}

/// Turns a URL or bare hostname into `scheme://host[:port]` (no trailing slash).
///
/// Bare hostnames get `https`, except `localhost`/loopback which get `http`.
pub fn normalize_base_url(input: &str) -> Option<String> {
    let input = input.trim();
    let (explicit_scheme, rest) = if let Some(rest) = strip_prefix_ci(input, "https://") {
        (Some("https"), rest)
    } else if let Some(rest) = strip_prefix_ci(input, "http://") {
        (Some("http"), rest)
    } else if input.contains("://") {
        return None; // some other scheme
    } else {
        (None, input)
    };

    let authority = rest.split(['/', '?', '#']).next()?;
    if !is_valid_authority(authority) {
        return None;
    }
    let authority = authority.to_ascii_lowercase();
    let scheme = explicit_scheme.unwrap_or(if is_loopback(&authority) {
        "http"
    } else {
        "https"
    });
    Some(format!("{scheme}://{authority}"))
}

fn strip_prefix_ci<'a>(input: &'a str, prefix: &str) -> Option<&'a str> {
    let head = input.get(..prefix.len())?;
    head.eq_ignore_ascii_case(prefix)
        .then(|| &input[prefix.len()..])
}

fn is_loopback(authority: &str) -> bool {
    authority.starts_with("localhost")
        || authority.starts_with("127.")
        || authority.starts_with("[::1]")
}

/// `host[:port]` made only of hostname/IP characters, with a numeric port.
fn is_valid_authority(authority: &str) -> bool {
    if authority.is_empty() || authority.len() > 261 {
        return false;
    }
    if !authority
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '.' | ':' | '[' | ']'))
    {
        return false;
    }
    if authority.starts_with('[') {
        return authority.contains(']'); // IPv6 literal
    }
    match authority.rsplit_once(':') {
        Some((host, port)) => {
            !host.is_empty()
                && !host.contains(':')
                && !port.is_empty()
                && port.len() <= 5
                && port.parse::<u16>().is_ok()
        }
        None => true,
    }
}

/// Settings needed to generate absolute URLs and scan the static directory.
#[derive(Debug, Clone)]
pub struct SiteSettings {
    /// Configured public origin, if any.
    pub public_url: Option<String>,
    /// Whether `X-Forwarded-Proto` may be used by the Host-header fallback.
    pub trust_proxy_headers: bool,
    pub static_root: PathBuf,
    pub index_file: String,
}

impl SiteSettings {
    /// Origin used for absolute URLs in the sitemap and `robots.txt`.
    ///
    /// Uses the configured public URL. If none is configured it falls back to
    /// the request's `Host` header (validated), which is convenient locally but
    /// client-controlled; production deployments should configure the URL.
    pub fn base_url(&self, headers: &HeaderMap, uri: &Uri) -> String {
        if let Some(url) = &self.public_url {
            return url.clone();
        }
        let host = headers
            .get(HOST)
            .and_then(|v| v.to_str().ok())
            .or_else(|| uri.authority().map(|a| a.as_str()))
            .filter(|h| is_valid_authority(h));
        let Some(host) = host else {
            return "http://localhost".to_owned();
        };
        let scheme = self
            .trust_proxy_headers
            .then(|| headers.get("x-forwarded-proto")?.to_str().ok())
            .flatten()
            .and_then(|v| v.split(',').next())
            .map(|v| v.trim().to_ascii_lowercase())
            .filter(|v| v == "http" || v == "https")
            .unwrap_or_else(|| "http".to_owned());
        format!("{scheme}://{}", host.to_ascii_lowercase())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn env(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let map: HashMap<String, String> = pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        move |k| map.get(k).cloned()
    }

    #[test]
    fn normalizes_urls_and_hostnames() {
        let n = |s| normalize_base_url(s);
        assert_eq!(
            n("https://MyApp.onrender.com/").as_deref(),
            Some("https://myapp.onrender.com")
        );
        assert_eq!(
            n("myapp.onrender.com").as_deref(),
            Some("https://myapp.onrender.com")
        );
        assert_eq!(
            n("http://example.com:8080/a/b?x=1#f").as_deref(),
            Some("http://example.com:8080")
        );
        assert_eq!(
            n("localhost:8080").as_deref(),
            Some("http://localhost:8080")
        );
        assert_eq!(n("ftp://example.com"), None);
        assert_eq!(n("exa mple.com"), None);
        assert_eq!(n("user@evil.com"), None);
        assert_eq!(n("example.com:99999"), None);
        assert_eq!(n(""), None);
    }

    #[test]
    fn render_variables_win_over_generic_ones() {
        let get = env(&[
            ("EXTERNAL_URL", "https://generic.example"),
            ("RENDER_EXTERNAL_HOSTNAME", "myapp.onrender.com"),
        ]);
        let r = resolve_public_url(None, get);
        assert_eq!(r.url.as_deref(), Some("https://myapp.onrender.com"));
        assert_eq!(r.source, Some("RENDER_EXTERNAL_HOSTNAME"));
    }

    #[test]
    fn url_variable_beats_hostname_variable_in_the_same_group() {
        let get = env(&[
            ("RENDER_EXTERNAL_URL", "https://a.onrender.com"),
            ("RENDER_EXTERNAL_HOSTNAME", "b.onrender.com"),
        ]);
        assert_eq!(
            resolve_public_url(None, get).url.as_deref(),
            Some("https://a.onrender.com")
        );
    }

    #[test]
    fn falls_back_to_generic_variables_then_nothing() {
        let r = resolve_public_url(None, env(&[("EXTERNAL_HOSTNAME", "blog.example.org")]));
        assert_eq!(
            (r.url.as_deref(), r.source),
            (Some("https://blog.example.org"), Some("EXTERNAL_HOSTNAME"))
        );
        assert_eq!(resolve_public_url(None, env(&[])).url, None);
    }

    #[test]
    fn invalid_and_empty_values_are_skipped_with_a_warning() {
        let get = env(&[
            ("RENDER_EXTERNAL_URL", "   "),
            ("RENDER_EXTERNAL_HOSTNAME", "not a host"),
            ("EXTERNAL_URL", "https://ok.example"),
        ]);
        let r = resolve_public_url(None, get);
        assert_eq!(r.url.as_deref(), Some("https://ok.example"));
        assert_eq!(r.warnings.len(), 1);
    }

    #[test]
    fn cli_flag_has_highest_priority() {
        let r = resolve_public_url(
            Some("https://cli.example"),
            env(&[("RENDER_EXTERNAL_URL", "https://env.example")]),
        );
        assert_eq!(
            (r.url.as_deref(), r.source),
            (Some("https://cli.example"), Some("--public-url"))
        );
    }

    #[test]
    fn host_header_fallback_is_validated() {
        let site = SiteSettings {
            public_url: None,
            trust_proxy_headers: true,
            static_root: PathBuf::new(),
            index_file: String::new(),
        };
        let uri = Uri::from_static("/sitemap.xml");
        let mut h = HeaderMap::new();
        h.insert(HOST, "Example.com:8080".parse().unwrap());
        h.insert("x-forwarded-proto", "https".parse().unwrap());
        assert_eq!(site.base_url(&h, &uri), "https://example.com:8080");
        h.insert(HOST, "evil.com/<script>".parse().unwrap());
        assert_eq!(site.base_url(&h, &uri), "http://localhost");
        let configured = SiteSettings {
            public_url: Some("https://real.example".into()),
            ..site
        };
        assert_eq!(configured.base_url(&h, &uri), "https://real.example");
    }
}
