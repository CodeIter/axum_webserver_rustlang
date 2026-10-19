//! Handlebars template engine wrapper.
//!
//! Directory layout (relative to the configured templates directory):
//!
//! ```text
//! layouts/layout.hbs        base document (registered as "layouts/layout")
//! partials/meta.hbs         <meta> tags     ({{> partials/meta}})
//! partials/navigation.hbs   navigation bar  ({{> partials/navigation}})
//! index.hbs, about.hbs, 404.hbs   page bodies
//! ```
//!
//! Rendering is two-step: the page template renders to an HTML fragment, which
//! is then passed to the layout as the (unescaped) `body` variable.

use std::{path::Path, sync::Arc};

use handlebars::{DirectorySourceOptions, Handlebars, RenderError};
use serde_json::{json, Map, Value};

/// Templates that must exist for the server to start.
const REQUIRED: &[&str] = &[
    DEFAULT_LAYOUT,
    HELLO_LAYOUT,
    "partials/meta",
    "partials/navigation",
    "partials/post_card",
    "partials/post_cards",
    "partials/load_more",
    "index",
    "about",
    "hello_world",
    "blog",
    "blog_post",
    "404",
];
/// Layout used by every page unless another one is requested.
pub const DEFAULT_LAYOUT: &str = "layouts/layout";
/// Dedicated layout of the `/hello/world` page.
pub const HELLO_LAYOUT: &str = "layouts/layout_hello";
const APP_NAME: &str = "Axum Server";

/// Failure while loading templates at startup.
#[derive(Debug, thiserror::Error)]
pub enum TemplateError {
    #[error("failed to load templates from `{dir}`: {source}")]
    Load {
        dir: String,
        #[source]
        source: Box<handlebars::TemplateError>,
    },
    #[error("missing required template `{0}` (expected `{0}.hbs` inside the templates directory)")]
    Missing(&'static str),
}

/// Cheaply cloneable, immutable template registry.
#[derive(Clone)]
pub struct Templates {
    engine: Arc<Handlebars<'static>>,
}

impl Templates {
    /// Loads every `*.hbs` file under `dir` and verifies the required ones exist.
    ///
    /// Template names are paths relative to `dir` without the extension.
    pub fn load(dir: &Path) -> Result<Self, TemplateError> {
        let mut engine = Handlebars::new();
        engine
            .register_templates_directory(dir, DirectorySourceOptions::default())
            .map_err(|source| TemplateError::Load {
                dir: dir.display().to_string(),
                source: Box::new(source),
            })?;

        if let Some(missing) = REQUIRED.iter().find(|name| !engine.has_template(name)) {
            return Err(TemplateError::Missing(missing));
        }
        Ok(Self {
            engine: Arc::new(engine),
        })
    }

    /// Renders a template (typically a partial) *without* the layout.
    pub fn render_fragment(&self, name: &str, data: &Value) -> Result<String, RenderError> {
        self.engine.render(name, data)
    }

    /// Renders `page` and wraps it in `layout` (e.g. [`DEFAULT_LAYOUT`]).
    ///
    /// `data` should be a JSON object (typically with `title` and
    /// `current_path`); `app_name` and `version` are added automatically.
    /// Values are HTML-escaped by default.
    pub fn render_page_in(
        &self,
        layout: &str,
        page: &str,
        data: Value,
    ) -> Result<String, RenderError> {
        let mut context: Map<String, Value> = match data {
            Value::Object(map) => map,
            _ => Map::new(),
        };
        context.entry("app_name").or_insert_with(|| json!(APP_NAME));
        context
            .entry("version")
            .or_insert_with(|| json!(env!("CARGO_PKG_VERSION")));

        let body = self.engine.render(page, &context)?;
        context.insert("body".into(), Value::String(body));
        self.engine.render(layout, &context)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn templates() -> Templates {
        // `cargo test` runs from the crate root.
        Templates::load(Path::new("templates")).expect("bundled templates load")
    }

    #[test]
    fn renders_page_inside_layout_with_partials() {
        let html = templates()
            .render_page_in(
                DEFAULT_LAYOUT,
                "index",
                json!({ "title": "Home", "current_path": "/" }),
            )
            .unwrap();
        assert!(html.contains("<!doctype html>"));
        assert!(html.contains("<meta charset=\"utf-8\">")); // meta partial
        assert!(html.contains("<nav")); // navigation partial
        assert!(html.contains("<title>Home | Axum Server</title>"));
        assert!(html.contains("Welcome to Axum Server")); // page body, unescaped
    }

    #[test]
    fn marks_current_navigation_item() {
        let html = templates()
            .render_page_in(
                DEFAULT_LAYOUT,
                "about",
                json!({ "title": "About", "current_path": "/about" }),
            )
            .unwrap();
        assert!(html.contains("href=\"/about\" aria-current=\"page\""));
        assert!(!html.contains("href=\"/\" aria-current=\"page\""));
    }

    #[test]
    fn escapes_untrusted_values() {
        let html = templates()
            .render_page_in(
                DEFAULT_LAYOUT,
                "404",
                json!({ "title": "<script>x</script>", "current_path": "" }),
            )
            .unwrap();
        assert!(!html.contains("<script>x</script>"));
        assert!(html.contains("&lt;script&gt;"));
    }

    #[test]
    fn hello_page_uses_its_dedicated_layout() {
        let t = templates();
        let data = json!({ "title": "Hello World", "current_path": "/hello/world" });
        let hello = t
            .render_page_in(HELLO_LAYOUT, "hello_world", data.clone())
            .unwrap();
        assert!(hello.contains("Hello, World!") && hello.contains("hello-body"));
        assert!(!hello.contains("<nav")); // no site navigation in this layout
        let default = t
            .render_page_in(DEFAULT_LAYOUT, "hello_world", data)
            .unwrap();
        assert!(default.contains("<nav") && !default.contains("hello-body"));
    }

    #[test]
    fn missing_directory_is_reported() {
        assert!(Templates::load(Path::new("does-not-exist")).is_err());
    }
}
