//! YAML front-matter extraction and *lenient* field parsing.
//!
//! A malformed or mistyped field never prevents a post from loading: it is
//! reported as a warning and the field falls back to its default.

use chrono::{DateTime, NaiveDate, NaiveDateTime, TimeZone, Utc};
use serde_yaml_ng::{Mapping, Value};

/// Splits `raw` into `(front_matter_yaml, markdown_body)`.
///
/// Front-matter is a block delimited by lines containing only `---` (the
/// closing delimiter may also be `...`) at the very start of the file. Without
/// a complete block the whole input is the body.
pub fn split(raw: &str) -> (Option<&str>, &str) {
    let raw = raw.strip_prefix('\u{feff}').unwrap_or(raw);
    let mut lines = raw.split_inclusive('\n');
    match lines.next() {
        Some(first) if first.trim_end() == "---" => {
            let start = first.len();
            let mut offset = start;
            for line in lines {
                let trimmed = line.trim_end();
                if trimmed == "---" || trimmed == "..." {
                    return (Some(&raw[start..offset]), &raw[offset + line.len()..]);
                }
                offset += line.len();
            }
            (None, raw) // unterminated block: treat everything as content
        }
        _ => (None, raw),
    }
}

/// Metadata declared by the author; `None` means "use the default".
#[derive(Debug, Default)]
pub struct FrontMatter {
    pub slug: Option<String>,
    pub title: Option<String>,
    pub excerpt: Option<String>,
    pub author: Option<Vec<String>>,
    pub license: Option<String>,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
    pub keywords: Option<Vec<String>>,
    pub category: Option<String>,
    pub serie: Option<String>,
    pub lang: Option<String>,
    pub image: Option<String>,
    pub image_alt: Option<String>,
    pub published: Option<bool>,
    pub featured: Option<bool>,
    pub canonical_url: Option<String>,
}

impl FrontMatter {
    /// Parses a YAML block. Returns the metadata plus human-readable warnings.
    ///
    /// `readingTime` is intentionally not read: it is always computed.
    pub fn parse(yaml: &str) -> (Self, Vec<String>) {
        let mut warnings = Vec::new();
        let value: Value = match serde_yaml_ng::from_str(yaml) {
            Ok(value) => value,
            Err(err) => {
                warnings.push(format!("invalid YAML front-matter ({err}); using defaults"));
                return (Self::default(), warnings);
            }
        };
        let map = match value {
            Value::Mapping(map) => map,
            Value::Null => return (Self::default(), warnings),
            _ => {
                warnings.push("front-matter is not a key/value mapping; using defaults".into());
                return (Self::default(), warnings);
            }
        };

        let fm = Self {
            slug: text(&map, "slug"),
            title: text(&map, "title"),
            excerpt: text(&map, "excerpt"),
            author: list(&map, "author", false),
            license: text(&map, "license"),
            created_at: date(&map, "createdAt", &mut warnings),
            updated_at: date(&map, "updatedAt", &mut warnings),
            keywords: list(&map, "keywords", true),
            category: text(&map, "category"),
            serie: text(&map, "serie"),
            lang: text(&map, "lang"),
            image: url_or_path(&map, "image", &mut warnings),
            image_alt: text(&map, "imageAlt"),
            published: boolean(&map, "published", &mut warnings),
            featured: boolean(&map, "featured", &mut warnings),
            canonical_url: url_or_path(&map, "canonicalUrl", &mut warnings),
        };
        (fm, warnings)
    }
}

/// A scalar YAML value as a trimmed, non-empty string.
fn scalar(value: &Value) -> Option<String> {
    let s = match value {
        Value::String(s) => s.trim().to_owned(),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        _ => return None,
    };
    (!s.is_empty()).then_some(s)
}

fn text(map: &Mapping, key: &str) -> Option<String> {
    map.get(key).and_then(scalar)
}

/// `string | array[string]`. With `split_commas`, a plain string such as
/// `"rust, web"` becomes several entries (used for keywords).
fn list(map: &Mapping, key: &str, split_commas: bool) -> Option<Vec<String>> {
    let items: Vec<String> = match map.get(key)? {
        Value::Sequence(seq) => seq.iter().filter_map(scalar).collect(),
        Value::String(s) if split_commas => s
            .split(',')
            .map(|part| part.trim().to_owned())
            .filter(|part| !part.is_empty())
            .collect(),
        other => scalar(other).into_iter().collect(),
    };
    (!items.is_empty()).then_some(items)
}

fn boolean(map: &Mapping, key: &str, warnings: &mut Vec<String>) -> Option<bool> {
    match map.get(key)? {
        Value::Bool(b) => Some(*b),
        Value::String(s) if s.trim().eq_ignore_ascii_case("true") => Some(true),
        Value::String(s) if s.trim().eq_ignore_ascii_case("false") => Some(false),
        Value::Null => None,
        _ => {
            warnings.push(format!("`{key}` must be true or false; using the default"));
            None
        }
    }
}

fn date(map: &Mapping, key: &str, warnings: &mut Vec<String>) -> Option<DateTime<Utc>> {
    let raw = text(map, key)?;
    let parsed = parse_datetime(&raw);
    if parsed.is_none() {
        warnings.push(format!(
            "`{key}` is not an ISO date or datetime ({raw:?}); using the file time"
        ));
    }
    parsed
}

/// Accepts `YYYY-MM-DD`, `YYYY-MM-DDTHH:MM[:SS[.f]]` (naive values are UTC) and RFC 3339.
pub fn parse_datetime(input: &str) -> Option<DateTime<Utc>> {
    let input = input.trim();
    if let Ok(dt) = DateTime::parse_from_rfc3339(input) {
        return Some(dt.with_timezone(&Utc));
    }
    for fmt in [
        "%Y-%m-%dT%H:%M:%S%.f",
        "%Y-%m-%d %H:%M:%S%.f",
        "%Y-%m-%dT%H:%M",
        "%Y-%m-%d %H:%M",
    ] {
        if let Ok(naive) = NaiveDateTime::parse_from_str(input, fmt) {
            return Some(Utc.from_utc_datetime(&naive));
        }
    }
    let day = NaiveDate::parse_from_str(input, "%Y-%m-%d").ok()?;
    Some(Utc.from_utc_datetime(&day.and_hms_opt(0, 0, 0)?))
}

/// Validates a URL or site path used in `href`/`src` attributes.
///
/// Allowed: `http(s)://...` and paths (`/img/a.png`; relative paths are rooted
/// at `/`). Rejected: other schemes (`javascript:`, `data:`) and `//host`.
pub fn normalize_url_or_path(input: &str) -> Option<String> {
    let input = input.trim();
    if input.is_empty() || input.chars().any(|c| c.is_control() || c.is_whitespace()) {
        return None;
    }
    let lower = input.to_ascii_lowercase();
    if lower.starts_with("http://") || lower.starts_with("https://") {
        return Some(input.to_owned());
    }
    if input.starts_with("//") {
        return None;
    }
    if input.starts_with('/') {
        return Some(input.to_owned());
    }
    // A colon before the first `/`, `?` or `#` means a URL scheme.
    let head = input.split(['/', '?', '#']).next().unwrap_or("");
    if head.contains(':') {
        return None;
    }
    Some(format!("/{input}"))
}

fn url_or_path(map: &Mapping, key: &str, warnings: &mut Vec<String>) -> Option<String> {
    let raw = text(map, key)?;
    let normalized = normalize_url_or_path(&raw);
    if normalized.is_none() {
        warnings.push(format!(
            "`{key}` must be an http(s) URL or a path ({raw:?}); ignoring it"
        ));
    }
    normalized
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_front_matter_with_crlf_and_bom() {
        let (fm, body) = split("\u{feff}---\r\ntitle: A\r\n---\r\nHello");
        assert_eq!(fm, Some("title: A\r\n"));
        assert_eq!(body, "Hello");
    }

    #[test]
    fn unterminated_or_absent_front_matter_is_content() {
        assert_eq!(split("---\ntitle: A\nHello").0, None);
        assert_eq!(split("Just text\n---\nmore").0, None);
    }

    #[test]
    fn parses_types_leniently() {
        let yaml = "author: [Ann, Bob]\nkeywords: rust, web ,\nfeatured: \"true\"\npublished: maybe\ncreatedAt: 2024-03-01\nupdatedAt: nonsense\nimage: javascript:alert(1)\n";
        let (fm, warnings) = FrontMatter::parse(yaml);
        assert_eq!(fm.author, Some(vec!["Ann".into(), "Bob".into()]));
        assert_eq!(fm.keywords, Some(vec!["rust".into(), "web".into()]));
        assert_eq!(fm.featured, Some(true));
        assert_eq!(fm.published, None);
        assert_eq!(
            fm.created_at.unwrap().to_rfc3339(),
            "2024-03-01T00:00:00+00:00"
        );
        assert!(fm.updated_at.is_none() && fm.image.is_none());
        assert_eq!(warnings.len(), 3);
    }

    #[test]
    fn invalid_yaml_falls_back_to_defaults() {
        let (fm, warnings) = FrontMatter::parse("title: [unclosed");
        assert!(fm.title.is_none());
        assert_eq!(warnings.len(), 1);
    }

    #[test]
    fn datetime_formats() {
        assert!(parse_datetime("2024-03-01T10:30:00+02:00").is_some());
        assert!(parse_datetime("2024-03-01 10:30").is_some());
        assert!(parse_datetime("03/01/2024").is_none());
    }

    #[test]
    fn url_normalization() {
        assert_eq!(
            normalize_url_or_path("img/a.png").as_deref(),
            Some("/img/a.png")
        );
        assert_eq!(
            normalize_url_or_path("https://x.io/a").as_deref(),
            Some("https://x.io/a")
        );
        assert!(normalize_url_or_path("//evil.com").is_none());
        assert!(normalize_url_or_path("data:text/html,x").is_none());
    }
}
