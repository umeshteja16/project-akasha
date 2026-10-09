//! Plain text, CSV, JSON and Markdown.

use pulldown_cmark::{Event, Options as MdOptions, Parser, Tag, TagEnd};

use crate::{Extraction, TextBuilder, normalize::normalize};

/// Decode bytes as UTF-8 (lossily: uploads are validated, but be forgiving here).
fn decode(bytes: &[u8]) -> std::borrow::Cow<'_, str> {
    String::from_utf8_lossy(bytes)
}

fn finish(extractor: &'static str, text: &str, max_chars: usize) -> Extraction {
    let mut builder = TextBuilder::new(max_chars);
    builder.push(&normalize(text));
    let mut notes = Vec::new();
    let text = builder.finish(&mut notes);
    Extraction {
        extractor,
        text,
        pages: Vec::new(),
        notes,
    }
}

/// Text kept as written (normalised). Used for plain text, CSV and JSON, whose
/// punctuation is meaningful to readers and harmless to the indexer.
pub(crate) fn plain(bytes: &[u8], extractor: &'static str, max_chars: usize) -> Extraction {
    finish(extractor, &decode(bytes), max_chars)
}

/// Markdown with the syntax removed: headings, paragraphs, list items, table rows
/// and code blocks become plain lines; links and images keep their text; raw HTML
/// is dropped.
pub(crate) fn markdown(bytes: &[u8], max_chars: usize) -> Extraction {
    finish("markdown", &strip_markdown(&decode(bytes)), max_chars)
}

pub(crate) fn strip_markdown(source: &str) -> String {
    let options = MdOptions::ENABLE_TABLES
        | MdOptions::ENABLE_STRIKETHROUGH
        | MdOptions::ENABLE_TASKLISTS
        | MdOptions::ENABLE_FOOTNOTES
        | MdOptions::ENABLE_YAML_STYLE_METADATA_BLOCKS;
    let mut out = String::with_capacity(source.len());
    let mut in_metadata = false;
    let mut row_start = false;
    for event in Parser::new_ext(source, options) {
        match event {
            Event::Start(Tag::MetadataBlock(_)) => in_metadata = true,
            Event::End(TagEnd::MetadataBlock(_)) => in_metadata = false,
            Event::Text(t) | Event::Code(t) | Event::InlineMath(t) | Event::DisplayMath(t)
                if !in_metadata =>
            {
                out.push_str(&t);
            }
            Event::SoftBreak | Event::HardBreak => out.push('\n'),
            Event::Start(Tag::Heading { .. } | Tag::CodeBlock(_) | Tag::BlockQuote(_)) => {
                out.push_str("\n\n");
            }
            Event::End(
                TagEnd::Heading(_)
                | TagEnd::Paragraph
                | TagEnd::CodeBlock
                | TagEnd::BlockQuote(_)
                | TagEnd::Table
                | TagEnd::FootnoteDefinition,
            )
            | Event::Rule => out.push_str("\n\n"),
            Event::Start(Tag::Item) | Event::End(TagEnd::TableHead | TagEnd::TableRow) => {
                out.push('\n');
            }
            Event::Start(Tag::TableHead | Tag::TableRow) => row_start = true,
            Event::Start(Tag::TableCell) => {
                if !row_start {
                    out.push_str(" | ");
                }
                row_start = false;
            }
            _ => {}
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markdown_keeps_words_and_headings_drops_syntax() {
        let md = "---\ntitle: secret front matter\n---\n# Title\n\nSome *emphasis* and a [link](https://x.y).\n\n\
                  - one\n- two\n\n```rust\nlet x = 1;\n```\n\n| a | b |\n|---|---|\n| 1 | 2 |\n\n<div>raw</div>\n";
        let text = crate::normalize::normalize(&strip_markdown(md));
        assert_eq!(
            text,
            "Title\n\nSome emphasis and a link.\n\none\ntwo\n\nlet x = 1;\n\na | b\n1 | 2"
        );
    }
}
