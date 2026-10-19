//! Markdown -> sanitized HTML, plus text statistics.

use pulldown_cmark::{html, Event, Options, Parser, Tag, TagEnd};

/// Result of rendering one post body.
pub struct Rendered {
    /// Sanitized HTML (safe to emit unescaped).
    pub html: String,
    /// Plain text of the first non-empty paragraph (may be empty).
    pub first_paragraph: String,
    /// Plain text of the whole document, whitespace-normalised.
    pub plain_text: String,
    pub word_count: usize,
}

/// Renders `markdown`.
///
/// Raw HTML inside the Markdown is allowed but passed through an allow-list
/// sanitizer (no scripts, event handlers or `javascript:` URLs).
pub fn render(markdown: &str, sanitizer: &ammonia::Builder<'_>) -> Rendered {
    let options = Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH;
    let events: Vec<Event<'_>> = Parser::new_ext(markdown, options).collect();

    let mut unsafe_html = String::new();
    html::push_html(&mut unsafe_html, events.iter().cloned());

    let mut plain = String::new();
    let mut first = String::new();
    let (mut in_image, mut in_first, mut first_done) = (0usize, false, false);

    for event in &events {
        match event {
            Event::Start(Tag::Image { .. }) => in_image += 1,
            Event::End(TagEnd::Image) => in_image = in_image.saturating_sub(1),
            Event::Start(Tag::Paragraph) if !first_done => in_first = true,
            Event::End(TagEnd::Paragraph) => {
                plain.push(' ');
                if in_first {
                    in_first = false;
                    first_done = !first.trim().is_empty();
                    if !first_done {
                        first.clear();
                    }
                }
            }
            // Image alt text is not part of the readable prose.
            Event::Text(t) | Event::Code(t) if in_image == 0 => {
                plain.push_str(t);
                if in_first {
                    first.push_str(t);
                }
            }
            Event::SoftBreak | Event::HardBreak => {
                plain.push(' ');
                if in_first {
                    first.push(' ');
                }
            }
            Event::End(
                TagEnd::Heading(_)
                | TagEnd::Item
                | TagEnd::CodeBlock
                | TagEnd::TableCell
                | TagEnd::BlockQuote(_),
            ) => plain.push(' '),
            _ => {}
        }
    }

    Rendered {
        html: sanitizer.clean(&unsafe_html).to_string(),
        first_paragraph: normalize_ws(&first),
        word_count: plain.split_whitespace().count(),
        plain_text: normalize_ws(&plain),
    }
}

/// Builds the HTML sanitizer used for every post.
pub fn sanitizer() -> ammonia::Builder<'static> {
    let mut builder = ammonia::Builder::default();
    // Keep syntax-highlighting hooks (`language-rust`) on fenced code blocks.
    builder.add_tag_attributes("code", &["class"]);
    builder
}

fn normalize_ws(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(md: &str) -> Rendered {
        render(md, &sanitizer())
    }

    #[test]
    fn extracts_first_paragraph_skipping_headings_and_images() {
        let r = run("# Title\n\n![alt text](/a.png)\n\nFirst *real* paragraph.\n\nSecond.");
        assert_eq!(r.first_paragraph, "First real paragraph.");
        assert!(!r.plain_text.contains("alt text"));
    }

    #[test]
    fn counts_words() {
        assert_eq!(run("one two\n\nthree **fo**ur").word_count, 4);
    }

    #[test]
    fn sanitizes_raw_html() {
        let r = run(
            "Hi <script>alert(1)</script><a href=\"javascript:alert(1)\" onclick=\"x()\">x</a>",
        );
        assert!(
            !r.html.contains("<script")
                && !r.html.contains("javascript:")
                && !r.html.contains("onclick")
        );
    }

    #[test]
    fn keeps_code_language_class_and_tables() {
        let r = run("```rust\nfn main() {}\n```\n\n| a | b |\n|---|---|\n| 1 | 2 |");
        assert!(r.html.contains("language-rust"));
        assert!(r.html.contains("<table>"));
    }
}
