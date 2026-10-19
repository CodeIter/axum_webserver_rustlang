//! Search-engine files: `sitemap.xml` and `robots.txt` content.

use std::{collections::HashSet, fs, path::Path};

use chrono::{DateTime, Utc};
use percent_encoding::utf8_percent_encode;

use crate::services::{blog::Blog, redirect::PATH_SEGMENT};

/// Protocol limit: a sitemap may list at most 50,000 URLs.
pub const MAX_URLS: usize = 50_000;
/// Directory nesting limit when scanning the static directory.
const MAX_SCAN_DEPTH: usize = 16;

/// One sitemap entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// Site-relative, already percent-encoded path (e.g. `/blog/2026-09-12/x`).
    pub path: String,
    pub lastmod: Option<DateTime<Utc>>,
}

/// Gathers entries: template pages, then blog posts, then static HTML files.
///
/// Duplicate paths are dropped (first wins) and the list is capped at [`MAX_URLS`].
pub fn collect(pages: &[&str], blog: &Blog, static_entries: Vec<Entry>) -> Vec<Entry> {
    let pages = pages.iter().map(|path| Entry {
        path: (*path).to_owned(),
        lastmod: None,
    });
    let posts = blog.posts().iter().map(|post| Entry {
        path: post.url(),
        lastmod: Some(post.updated_at),
    });

    let mut seen = HashSet::new();
    pages
        .chain(posts)
        .chain(static_entries)
        .filter(|entry| seen.insert(entry.path.clone()))
        .take(MAX_URLS)
        .collect()
}

/// Lists public `.html` / `.htm` files below `root`.
///
/// * Hidden files and directories (names starting with `.`) are never listed.
/// * Symbolic links are not followed.
/// * `<dir>/<index_file>` is listed as `/<dir>/`; the root index file is skipped
///   because `/` is already a template route.
pub fn static_html_entries(root: &Path, index_file: &str) -> Vec<Entry> {
    let mut entries = Vec::new();
    scan(root, &mut Vec::new(), index_file, &mut entries);
    entries.sort_by(|a, b| a.path.cmp(&b.path));
    entries
}

fn scan(dir: &Path, segments: &mut Vec<String>, index_file: &str, out: &mut Vec<Entry>) {
    if segments.len() > MAX_SCAN_DEPTH {
        return;
    }
    let Ok(read_dir) = fs::read_dir(dir) else {
        return;
    };
    for entry in read_dir.flatten() {
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
            continue; // non-UTF-8 names cannot be expressed as URLs
        };
        if name.starts_with('.') {
            continue;
        }
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_dir() {
            segments.push(name);
            scan(&entry.path(), segments, index_file, out);
            segments.pop();
        } else if file_type.is_file() && is_html(&name) {
            let path = if name == index_file {
                if segments.is_empty() {
                    continue;
                }
                format!("/{}/", encode_segments(segments))
            } else {
                let mut all = segments.clone();
                all.push(name);
                format!("/{}", encode_segments(&all))
            };
            let lastmod = entry
                .metadata()
                .and_then(|m| m.modified())
                .ok()
                .map(DateTime::<Utc>::from);
            out.push(Entry { path, lastmod });
        }
    }
}

fn is_html(name: &str) -> bool {
    Path::new(name)
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("html") || e.eq_ignore_ascii_case("htm"))
}

fn encode_segments(segments: &[String]) -> String {
    segments
        .iter()
        .map(|s| utf8_percent_encode(s, PATH_SEGMENT).to_string())
        .collect::<Vec<_>>()
        .join("/")
}

/// Renders a sitemap (<https://www.sitemaps.org/protocol.html>) with absolute URLs.
pub fn render_sitemap(base_url: &str, entries: &[Entry]) -> String {
    let base = base_url.trim_end_matches('/');
    let mut xml = String::with_capacity(128 + entries.len() * 120);
    xml.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    xml.push_str("<urlset xmlns=\"http://www.sitemaps.org/schemas/sitemap/0.9\">\n");
    for entry in entries {
        xml.push_str("  <url>\n    <loc>");
        xml.push_str(&xml_escape(&format!("{base}{}", entry.path)));
        xml.push_str("</loc>\n");
        if let Some(lastmod) = entry.lastmod {
            xml.push_str("    <lastmod>");
            xml.push_str(&lastmod.format("%Y-%m-%dT%H:%M:%SZ").to_string());
            xml.push_str("</lastmod>\n");
        }
        xml.push_str("  </url>\n");
    }
    xml.push_str("</urlset>\n");
    xml
}

/// Escapes the five XML special characters.
fn xml_escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(c),
        }
    }
    out
}

/// Renders `robots.txt`: crawl everything except the JSON API, and point to the sitemap.
pub fn render_robots(base_url: &str) -> String {
    format!(
        "User-agent: *\nAllow: /\nDisallow: /api/\n\nSitemap: {}/sitemap.xml\n",
        base_url.trim_end_matches('/')
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn touch(root: &Path, rel: &str) {
        let path = root.join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, "<html></html>").unwrap();
    }

    #[test]
    fn scans_only_public_html_files() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        for f in [
            "index.html", // root index: `/` is a template route
            "page.html",
            "docs/index.html", // -> /docs/
            "docs/guide.HTM",
            "my page.html",     // needs encoding
            ".hidden.html",     // hidden file
            ".git/secret.html", // hidden directory
            "css/style.css",    // not HTML
            "a/b/deep.html",
        ] {
            touch(root, f);
        }
        let paths: Vec<String> = static_html_entries(root, "index.html")
            .into_iter()
            .map(|e| e.path)
            .collect();
        assert_eq!(
            paths,
            [
                "/a/b/deep.html",
                "/docs/",
                "/docs/guide.HTM",
                "/my%20page.html",
                "/page.html"
            ]
        );
    }

    #[test]
    fn missing_directory_yields_nothing() {
        assert!(static_html_entries(Path::new("/definitely/not/here"), "index.html").is_empty());
    }

    #[test]
    fn sitemap_xml_is_escaped_and_absolute() {
        let t = DateTime::parse_from_rfc3339("2026-09-12T10:30:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let xml = render_sitemap(
            "https://example.com/",
            &[
                Entry {
                    path: "/".into(),
                    lastmod: None,
                },
                Entry {
                    path: "/a?x=1&y=<2>".into(),
                    lastmod: Some(t),
                },
            ],
        );
        assert!(xml.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<urlset xmlns="));
        assert!(xml.contains("<loc>https://example.com/</loc>"));
        assert!(xml.contains("<loc>https://example.com/a?x=1&amp;y=&lt;2&gt;</loc>"));
        assert!(xml.contains("<lastmod>2026-09-12T10:30:00Z</lastmod>"));
        assert_eq!(xml.matches("<lastmod>").count(), 1);
        assert!(xml.trim_end().ends_with("</urlset>"));
    }

    #[test]
    fn collect_deduplicates_and_orders_sources() {
        let entries = collect(
            &["/", "/about"],
            &Blog::default(),
            vec![
                Entry {
                    path: "/about".into(),
                    lastmod: None,
                },
                Entry {
                    path: "/x.html".into(),
                    lastmod: None,
                },
            ],
        );
        let paths: Vec<&str> = entries.iter().map(|e| e.path.as_str()).collect();
        assert_eq!(paths, ["/", "/about", "/x.html"]);
    }

    #[test]
    fn robots_points_to_the_sitemap() {
        let robots = render_robots("https://example.com/");
        assert!(robots.starts_with("User-agent: *\n"));
        assert!(robots.contains("Disallow: /api/\n"));
        assert!(robots.ends_with("Sitemap: https://example.com/sitemap.xml\n"));
    }
}
