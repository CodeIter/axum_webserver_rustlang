//! Blog pages and the "load more" JSON endpoint.

use axum::{
    extract::{
        rejection::{PathRejection, QueryRejection},
        Path, Query, State,
    },
    http::StatusCode,
    response::Response,
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tracing::error;

use crate::{
    error::ApiError,
    handlers::pages::{not_found_page, render},
    services::blog::Listing,
    state::AppState,
};

/// Posts shown on `/blog` (and fetched per "Load more" click there).
const BLOG_PAGE_SIZE: usize = 10;
/// Upper bound for the `limit` query parameter of the API.
const MAX_PAGE_SIZE: usize = 50;

/// Template context shared by the home page and `/blog`.
pub fn listing_context(title: &str, current_path: &str, listing: &Listing) -> Value {
    json!({
        "title": title,
        "current_path": current_path,
        "posts": listing.posts,
        "total": listing.total,
        "has_more": listing.has_more,
        "next_offset": listing.next_offset,
        "page_size": listing.limit,
    })
}

/// `GET /blog` - the latest posts.
pub async fn index(State(state): State<AppState>) -> Response {
    let listing = state.blog.listing(0, BLOG_PAGE_SIZE);
    let data = listing_context("Blog", "/blog", &listing);
    render(&state.templates, StatusCode::OK, "blog", data)
}

/// `GET /blog/{createdatdate}/{slug}` - a single post (`404` if unknown or unpublished).
pub async fn post(
    State(state): State<AppState>,
    path: Result<Path<(String, String)>, PathRejection>,
) -> Response {
    let Ok(Path((date, slug))) = path else {
        return not_found_page(&state.templates);
    };
    // Slugs are stored lower-case, so make lookups case-insensitive.
    let Some(post) = state.blog.get(&date, &slug.to_lowercase()) else {
        return not_found_page(&state.templates);
    };

    // Other posts of the same series, in reading order (only if there are several).
    let series = (!post.serie.is_empty())
        .then(|| {
            let items: Vec<Value> = state
                .blog
                .series(&post.serie)
                .into_iter()
                .map(|p| json!({ "title": p.title, "url": p.url(), "current": p.url() == post.url() }))
                .collect();
            (items.len() > 1).then(|| json!({ "name": post.serie, "posts": items }))
        })
        .flatten();

    let data = json!({
        "title": post.title,
        "current_path": "/blog",
        "lang": post.lang,
        // Consumed by the `meta` partial for SEO tags.
        "description": post.excerpt,
        "keywords_meta": post.keywords.join(", "),
        "meta_author": post.authors.join(", "),
        "canonical_url": post.canonical_url,
        "og_type": "article",
        "post": post.view(true),
        "series": series,
    });
    render(&state.templates, StatusCode::OK, "blog_post", data)
}

/// Query string of [`posts`].
#[derive(Deserialize)]
pub struct PostsQuery {
    offset: Option<usize>,
    limit: Option<usize>,
}

/// Response of [`posts`]: the page plus ready-to-insert HTML cards.
#[derive(Serialize)]
pub struct PostsResponse {
    #[serde(flatten)]
    listing: Listing,
    /// Server-rendered cards (same partial as the pages use).
    html: String,
}

/// `GET /api/v1/blog/posts?offset=0&limit=10` - newest-first pagination.
pub async fn posts(
    State(state): State<AppState>,
    query: Result<Query<PostsQuery>, QueryRejection>,
) -> Result<Json<PostsResponse>, ApiError> {
    let Query(PostsQuery { offset, limit }) = query?;
    let limit = limit.unwrap_or(BLOG_PAGE_SIZE).clamp(1, MAX_PAGE_SIZE);
    let listing = state.blog.listing(offset.unwrap_or(0), limit);

    let html = state
        .templates
        .render_fragment("partials/post_cards", &json!({ "posts": listing.posts }))
        .map_err(|err| {
            error!(%err, "failed to render post cards");
            ApiError::Internal
        })?;
    Ok(Json(PostsResponse { listing, html }))
}
