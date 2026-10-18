//! Markdown blog: loads posts from a directory at startup and serves them
//! from memory.
//!
//! * [`front_matter`] parses the YAML header leniently.
//! * [`markdown`] renders and sanitizes the body.
//! * This module applies defaults, builds [`Post`]s and indexes them in [`Blog`].
//!
//! Posts are read once at startup; restart the server to pick up changes.

pub mod front_matter;
pub mod markdown;

use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    time::SystemTime,
};

use chrono::{DateTime, Utc};
use percent_encoding::utf8_percent_encode;
use serde::Serialize;
use tracing::{info, warn};

use crate::services::redirect::PATH_SEGMENT;
use front_matter::FrontMatter;

/// Maximum excerpt length (characters) when derived from the content.
const EXCERPT_CHARS: usize = 160;
/// Average adult reading speed used for `readingTime`.
const WORDS_PER_MINUTE: usize = 200;

/// A fully resolved post (front-matter merged with defaults).
#[derive(Debug, Clone)]
pub struct Post {
    pub slug: String,
    pub title: String,
    pub excerpt: String,
    pub authors: Vec<String>,
    pub license: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub keywords: Vec<String>,
    pub category: String,
    pub serie: String,
    pub lang: String,
    pub image: String,
    pub image_alt: String,
    pub published: bool,
    pub featured: bool,
    pub canonical_url: String,
    /// Estimated minutes, always computed from the content (minimum 1).
    pub reading_time: usize,
    /// Sanitized HTML body.
    pub html: String,
}

impl Post {
    /// `YYYY-MM-DD` of `created_at` (UTC): the `{createdatdate}` URL segment.
    pub fn url_date(&self) -> String {
        self.created_at.format("%Y-%m-%d").to_string()
    }

    /// Site-relative URL: `/blog/{createdatdate}/{slug}`.
    pub fn url(&self) -> String {
        format!("/blog/{}/{}", self.url_date(), utf8_percent_encode(&self.slug, PATH_SEGMENT))
    }

    /// Builds a post from raw file content.
    ///
    /// `stem` is the file name without extension; `fs_created` / `fs_modified`
    /// are the defaults for `createdAt` / `updatedAt`. Returns the post plus
    /// front-matter warnings.
    pub fn from_source(
        stem: &str,
        raw: &str,
        fs_created: DateTime<Utc>,
        fs_modified: DateTime<Utc>,
        sanitizer: &ammonia::Builder<'_>,
    ) -> (Self, Vec<String>) {
        let (yaml, body) = front_matter::split(raw);
        let (fm, warnings) = yaml.map(FrontMatter::parse).unwrap_or_default();
        let rendered = markdown::render(body, sanitizer);

        let slug = fm
            .slug
            .as_deref()
            .map(slugify)
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| {
                let s = slugify(stem);
                if s.is_empty() { "post".to_owned() } else { s }
            });

        let post = Self {
            title: fm.title.unwrap_or_else(|| title_from_slug(&slug)),
            excerpt: fm
                .excerpt
                .unwrap_or_else(|| make_excerpt(&rendered.first_paragraph, &rendered.plain_text)),
            authors: fm.author.unwrap_or_else(|| vec!["Anonymous".into()]),
            license: fm.license.unwrap_or_else(|| "CC BY-SA".into()),
            created_at: fm.created_at.unwrap_or(fs_created),
            updated_at: fm.updated_at.unwrap_or(fs_modified),
            keywords: fm.keywords.unwrap_or_default(),
            category: fm.category.unwrap_or_default(),
            serie: fm.serie.unwrap_or_default(),
            lang: fm.lang.unwrap_or_else(|| "en".into()),
            image: fm.image.unwrap_or_default(),
            image_alt: fm.image_alt.unwrap_or_default(),
            published: fm.published.unwrap_or(true),
            featured: fm.featured.unwrap_or(false),
            canonical_url: fm.canonical_url.unwrap_or_default(),
            reading_time: rendered.word_count.div_ceil(WORDS_PER_MINUTE).max(1),
            html: rendered.html,
            slug,
        };
        (post, warnings)
    }

    /// Template/JSON representation; the body is included only on request.
    pub fn view(&self, with_body: bool) -> PostView {
        PostView {
            slug: self.slug.clone(),
            url: self.url(),
            title: self.title.clone(),
            excerpt: self.excerpt.clone(),
            author: self.authors.join(", "),
            authors: self.authors.clone(),
            license: self.license.clone(),
            created_at: self.created_at.to_rfc3339(),
            created_date: self.url_date(),
            created_human: human_date(&self.created_at),
            updated_at: self.updated_at.to_rfc3339(),
            updated_date: self.updated_at.format("%Y-%m-%d").to_string(),
            updated_human: human_date(&self.updated_at),
            // Only worth showing when it is a different day than publication.
            show_updated: self.updated_at.date_naive() != self.created_at.date_naive(),
            keywords: self.keywords.clone(),
            category: self.category.clone(),
            serie: self.serie.clone(),
            lang: self.lang.clone(),
            image: self.image.clone(),
            image_alt: self.image_alt.clone(),
            featured: self.featured,
            canonical_url: self.canonical_url.clone(),
            reading_time: self.reading_time,
            html: with_body.then(|| self.html.clone()),
        }
    }
}

fn human_date(dt: &DateTime<Utc>) -> String {
    dt.format("%B %-d, %Y").to_string()
}

/// Serializable post view (snake_case keys) for templates and the JSON API.
#[derive(Debug, Serialize)]
pub struct PostView {
    pub slug: String,
    pub url: String,
    pub title: String,
    pub excerpt: String,
    /// Authors joined with ", ".
    pub author: String,
    pub authors: Vec<String>,
    pub license: String,
    pub created_at: String,
    pub created_date: String,
    pub created_human: String,
    pub updated_at: String,
    pub updated_date: String,
    pub updated_human: String,
    pub show_updated: bool,
    pub keywords: Vec<String>,
    pub category: String,
    pub serie: String,
    pub lang: String,
    pub image: String,
    pub image_alt: String,
    pub featured: bool,
    pub canonical_url: String,
    pub reading_time: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub html: Option<String>,
}

/// One page of the newest-first post list.
#[derive(Debug, Serialize)]
pub struct Listing {
    pub total: usize,
    pub offset: usize,
    pub limit: usize,
    pub has_more: bool,
    pub next_offset: usize,
    pub posts: Vec<PostView>,
}

/// All published posts, newest first.
#[derive(Debug, Default)]
pub struct Blog {
    posts: Vec<Post>,
    by_url: HashMap<(String, String), usize>,
}

impl Blog {
    /// Loads every `.md` / `.markdown` file under `dir` (recursively).
    ///
    /// A missing directory yields an empty blog (with a warning); unreadable
    /// files are skipped. Unpublished posts are dropped.
    pub fn load(dir: &Path) -> Self {
        let mut files = Vec::new();
        collect_markdown_files(dir, &mut files);
        files.sort();
        if files.is_empty() {
            warn!(dir = %dir.display(), "no blog posts found");
        }

        let sanitizer = markdown::sanitizer();
        let mut posts = Vec::new();
        for path in &files {
            let Ok(raw) = fs::read_to_string(path).map_err(|err| {
                warn!(file = %path.display(), %err, "cannot read blog post; skipping");
            }) else {
                continue;
            };
            let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("post");
            let (created, modified) = file_times(path);
            let (post, warnings) = Post::from_source(stem, &raw, created, modified, &sanitizer);
            for warning in warnings {
                warn!(file = %path.display(), "{warning}");
            }
            if post.published {
                posts.push(post);
            }
        }

        let blog = Self::from_posts(posts);
        info!(dir = %dir.display(), posts = blog.len(), "blog loaded");
        blog
    }

    /// Sorts newest first and indexes by `(date, slug)`; duplicates are dropped.
    pub fn from_posts(mut posts: Vec<Post>) -> Self {
        posts.sort_by(|a, b| b.created_at.cmp(&a.created_at).then_with(|| a.slug.cmp(&b.slug)));
        let mut by_url = HashMap::new();
        let mut unique = Vec::with_capacity(posts.len());
        for post in posts {
            let key = (post.url_date(), post.slug.clone());
            if by_url.contains_key(&key) {
                warn!(url = %post.url(), "duplicate blog post URL; ignoring the later file");
                continue;
            }
            by_url.insert(key, unique.len());
            unique.push(post);
        }
        Self { posts: unique, by_url }
    }

    pub fn len(&self) -> usize {
        self.posts.len()
    }

    /// Looks up a post by its URL date (`YYYY-MM-DD`) and slug.
    pub fn get(&self, date: &str, slug: &str) -> Option<&Post> {
        self.by_url
            .get(&(date.to_owned(), slug.to_owned()))
            .map(|&i| &self.posts[i])
    }

    /// Newest-first page of posts.
    pub fn listing(&self, offset: usize, limit: usize) -> Listing {
        let total = self.posts.len();
        let start = offset.min(total);
        let end = start.saturating_add(limit).min(total);
        Listing {
            total,
            offset: start,
            limit,
            has_more: end < total,
            next_offset: end,
            posts: self.posts[start..end].iter().map(|p| p.view(false)).collect(),
        }
    }

    /// Posts of one series, oldest first (reading order).
    pub fn series(&self, name: &str) -> Vec<&Post> {
        let mut found: Vec<&Post> = self.posts.iter().filter(|p| p.serie == name).collect();
        found.reverse(); // `posts` is newest first
        found
    }
}

fn collect_markdown_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        let hidden = entry.file_name().to_string_lossy().starts_with('.');
        if hidden {
            continue;
        }
        if path.is_dir() {
            collect_markdown_files(&path, out);
        } else if path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("md") || e.eq_ignore_ascii_case("markdown"))
        {
            out.push(path);
        }
    }
}

/// `(created, modified)` from the filesystem. Not every filesystem records a
/// creation time; the modification time (or "now") is used as a fallback.
fn file_times(path: &Path) -> (DateTime<Utc>, DateTime<Utc>) {
    let meta = fs::metadata(path).ok();
    let modified = meta.as_ref().and_then(|m| m.modified().ok()).unwrap_or_else(SystemTime::now);
    let created = meta.as_ref().and_then(|m| m.created().ok()).unwrap_or(modified);
    (created.into(), modified.into())
}

/// Lower-case, URL-safe identifier: alphanumerics kept, runs of anything else
/// collapsed into a single `-`, no leading/trailing `-`.
pub fn slugify(input: &str) -> String {
    let mut slug = String::new();
    for c in input.chars() {
        if c.is_alphanumeric() {
            slug.extend(c.to_lowercase());
        } else if !slug.ends_with('-') && !slug.is_empty() {
            slug.push('-');
        }
    }
    slug.trim_end_matches('-').to_owned()
}

/// `my-first_post` -> `My First Post`.
pub fn title_from_slug(slug: &str) -> String {
    slug.split(['-', '_'])
        .filter(|w| !w.is_empty())
        .map(|word| {
            let mut chars = word.chars();
            chars
                .next()
                .map(|first| first.to_uppercase().chain(chars).collect::<String>())
                .unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// First paragraph, capped at [`EXCERPT_CHARS`]; falls back to the start of the text.
fn make_excerpt(first_paragraph: &str, plain_text: &str) -> String {
    let source = if first_paragraph.is_empty() { plain_text } else { first_paragraph };
    if source.chars().count() <= EXCERPT_CHARS {
        return source.to_owned();
    }
    let cut: String = source.chars().take(EXCERPT_CHARS).collect();
    // Prefer breaking at a word boundary unless that would lose too much text.
    let trimmed = match cut.rfind(' ') {
        Some(i) if i > EXCERPT_CHARS / 2 => &cut[..i],
        _ => &cut,
    };
    format!("{}…", trimmed.trim_end_matches(|c: char| c.is_ascii_punctuation() || c.is_whitespace()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn ts(y: i32, m: u32, d: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(y, m, d, 12, 0, 0).unwrap()
    }

    fn build(stem: &str, raw: &str) -> Post {
        Post::from_source(stem, raw, ts(2024, 1, 1), ts(2024, 2, 2), &markdown::sanitizer()).0
    }

    #[test]
    fn applies_every_default() {
        let p = build("my-first_post", "Hello world.\n\nSecond paragraph.");
        assert_eq!(p.slug, "my-first-post");
        assert_eq!(p.title, "My First Post");
        assert_eq!(p.excerpt, "Hello world.");
        assert_eq!(p.authors, vec!["Anonymous"]);
        assert_eq!(p.license, "CC BY-SA");
        assert_eq!((p.created_at, p.updated_at), (ts(2024, 1, 1), ts(2024, 2, 2)));
        assert!(p.keywords.is_empty() && p.category.is_empty() && p.serie.is_empty());
        assert_eq!((p.lang.as_str(), p.image.as_str(), p.image_alt.as_str()), ("en", "", ""));
        assert_eq!((p.published, p.featured), (true, false));
        assert!(p.canonical_url.is_empty());
        assert_eq!(p.reading_time, 1);
    }

    #[test]
    fn front_matter_overrides_defaults_and_reading_time_is_computed() {
        let words = "word ".repeat(450);
        let raw = format!(
            "---\nslug: Custom Slug!\ntitle: Hi\nauthor: [A, B]\ncreatedAt: 2023-05-06\nreadingTime: 99\nfeatured: true\n---\n{words}"
        );
        let p = build("ignored", &raw);
        assert_eq!((p.slug.as_str(), p.title.as_str()), ("custom-slug", "Hi"));
        assert_eq!(p.authors.len(), 2);
        assert_eq!(p.url(), "/blog/2023-05-06/custom-slug");
        assert_eq!(p.reading_time, 3); // 450 words / 200 wpm, rounded up; front-matter value ignored
        assert!(p.featured);
    }

    #[test]
    fn long_first_paragraph_is_truncated() {
        let p = build("x", &format!("{}\n\nnext", "lorem ipsum ".repeat(40)));
        assert!(p.excerpt.chars().count() <= EXCERPT_CHARS + 1);
        assert!(p.excerpt.ends_with('…'));
    }

    #[test]
    fn excerpt_falls_back_to_text_when_no_paragraph() {
        assert_eq!(build("x", "# Only a heading").excerpt, "Only a heading");
    }

    #[test]
    fn index_orders_newest_first_pages_and_hides_nothing_extra() {
        let posts = (1..=7)
            .map(|d| {
                let raw = format!("---\ncreatedAt: 2024-01-0{d}\nserie: s\n---\nbody");
                build(&format!("p{d}"), &raw)
            })
            .collect();
        let blog = Blog::from_posts(posts);
        let first = blog.listing(0, 5);
        assert_eq!((first.total, first.posts.len(), first.has_more, first.next_offset), (7, 5, true, 5));
        assert_eq!(first.posts[0].slug, "p7");
        let rest = blog.listing(5, 5);
        assert_eq!((rest.posts.len(), rest.has_more), (2, false));
        assert!(blog.listing(99, 5).posts.is_empty());
        assert_eq!(blog.series("s").first().map(|p| p.slug.as_str()), Some("p1"));
        assert!(blog.get("2024-01-03", "p3").is_some());
        assert!(blog.get("2024-01-04", "p3").is_none());
    }

    #[test]
    fn slug_helpers() {
        assert_eq!(slugify("  Hello,  World_2! "), "hello-world-2");
        assert_eq!(title_from_slug("hello-world_2"), "Hello World 2");
    }
}
