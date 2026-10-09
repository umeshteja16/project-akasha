//! Snippets: `ts_headline` output (query terms between private-use markers, see
//! [`akasha_db::search::HIGHLIGHT_START`]) turned into plain text plus
//! highlight offsets, so clients never render HTML from the server.

use akasha_db::search::{HIGHLIGHT_END, HIGHLIGHT_START};

use crate::types::{Highlight, Snippet};

/// Strip the markers from `headline` and record the spans they enclosed (in
/// characters of the result). Unbalanced markers are dropped; empty spans and
/// adjacent spans are merged away.
pub fn parse(headline: &str) -> Snippet {
    let mut text = String::with_capacity(headline.len());
    let mut highlights: Vec<Highlight> = Vec::new();
    let mut chars: u32 = 0;
    let mut open: Option<u32> = None;
    for ch in headline.chars() {
        match ch {
            HIGHLIGHT_START => {
                open.get_or_insert(chars);
            }
            HIGHLIGHT_END => {
                if let Some(start) = open.take() {
                    push(&mut highlights, start, chars);
                }
            }
            _ => {
                text.push(ch);
                chars = chars.saturating_add(1);
            }
        }
    }
    if let Some(start) = open {
        push(&mut highlights, start, chars);
    }
    Snippet { text, highlights }
}

fn push(highlights: &mut Vec<Highlight>, start: u32, end: u32) {
    if end <= start {
        return;
    }
    match highlights.last_mut() {
        Some(last) if last.end == start => last.end = end,
        _ => highlights.push(Highlight { start, end }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spans(s: &Snippet) -> Vec<&str> {
        let chars: Vec<char> = s.text.chars().collect();
        s.highlights
            .iter()
            .map(|h| {
                let start = chars[..h.start as usize].iter().collect::<String>().len();
                let end = chars[..h.end as usize].iter().collect::<String>().len();
                &s.text[start..end]
            })
            .collect()
    }

    #[test]
    fn markers_become_character_offsets() {
        let s = parse("quick \u{E000}foxes\u{E001} jumped … the \u{E000}fox\u{E001}");
        assert_eq!(s.text, "quick foxes jumped … the fox");
        assert_eq!(spans(&s), vec!["foxes", "fox"]);
        assert_eq!(s.highlights[0], Highlight { start: 6, end: 11 });
    }

    #[test]
    fn offsets_count_characters_not_bytes() {
        let s = parse("naïve café \u{E000}crème\u{E001}");
        assert_eq!(s.highlights, vec![Highlight { start: 11, end: 16 }]);
        assert_eq!(spans(&s), vec!["crème"]);
    }

    #[test]
    fn malformed_markers_are_tolerated() {
        let s = parse("\u{E001}a \u{E000}\u{E001}b \u{E000}c\u{E001}\u{E000}d\u{E001} \u{E000}e");
        assert_eq!(s.text, "a b cd e");
        assert_eq!(spans(&s), vec!["cd", "e"]);
        assert_eq!(parse("plain").highlights, vec![]);
    }
}
