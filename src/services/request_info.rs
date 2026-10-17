//! Helpers for describing the incoming request (headers, forwarding info).

use std::{borrow::Cow, collections::BTreeMap};

use axum::http::HeaderMap;
use serde::Serialize;

/// One header's value(s): a string, or an array when the header was repeated.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(untagged)]
pub enum HeaderValues {
    One(String),
    Many(Vec<String>),
}

/// Lossily converts a header value (which may be non-UTF-8) to a string.
fn value_to_string(value: &axum::http::HeaderValue) -> Cow<'_, str> {
    String::from_utf8_lossy(value.as_bytes())
}

/// Collects every header into a name-sorted map. Names are lowercase.
pub fn collect_headers(headers: &HeaderMap) -> BTreeMap<String, HeaderValues> {
    headers
        .keys()
        .map(|name| {
            let mut values: Vec<String> = headers
                .get_all(name)
                .iter()
                .map(|v| value_to_string(v).into_owned())
                .collect();
            let entry = if values.len() == 1 {
                HeaderValues::One(values.remove(0))
            } else {
                HeaderValues::Many(values)
            };
            (name.as_str().to_owned(), entry)
        })
        .collect()
}

/// Raw proxy-forwarding headers exactly as received (`null` when absent).
///
/// These are **unverified, client-influenced** values; use
/// [`crate::services::client_ip`] for a trusted client address.
#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct ForwardingHeaders {
    pub cf_connecting_ip: Option<String>,
    pub true_client_ip: Option<String>,
    pub x_forwarded_for: Option<String>,
}

/// Returns a header's value; repeated header lines are joined with `", "`
/// (the HTTP list-folding rule, which is what `X-Forwarded-For` uses).
fn joined(headers: &HeaderMap, name: &str) -> Option<String> {
    let values: Vec<Cow<'_, str>> = headers.get_all(name).iter().map(value_to_string).collect();
    (!values.is_empty()).then(|| values.join(", "))
}

pub fn forwarding_headers(headers: &HeaderMap) -> ForwardingHeaders {
    ForwardingHeaders {
        cf_connecting_ip: joined(headers, "cf-connecting-ip"),
        true_client_ip: joined(headers, "true-client-ip"),
        x_forwarded_for: joined(headers, "x-forwarded-for"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map(pairs: &[(&'static str, &str)]) -> HeaderMap {
        let mut m = HeaderMap::new();
        for (k, v) in pairs {
            m.append(*k, v.parse().unwrap());
        }
        m
    }

    #[test]
    fn repeated_headers_become_arrays() {
        let out = collect_headers(&map(&[("accept", "a"), ("x-test", "1"), ("x-test", "2")]));
        assert_eq!(out["accept"], HeaderValues::One("a".into()));
        assert_eq!(out["x-test"], HeaderValues::Many(vec!["1".into(), "2".into()]));
    }

    #[test]
    fn forwarding_headers_join_lines_and_report_absence() {
        let f = forwarding_headers(&map(&[("x-forwarded-for", "1.1.1.1"), ("x-forwarded-for", "2.2.2.2, 3.3.3.3")]));
        assert_eq!(f.x_forwarded_for.as_deref(), Some("1.1.1.1, 2.2.2.2, 3.3.3.3"));
        assert_eq!(f.cf_connecting_ip, None);
        assert_eq!(f.true_client_ip, None);
    }
}
