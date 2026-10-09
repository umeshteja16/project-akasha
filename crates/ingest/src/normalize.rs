//! Text normalisation for indexing: NFC, `\n` line ends, no control characters,
//! single spaces, at most one blank line in a row, no trailing whitespace.

use unicode_normalization::UnicodeNormalization;

/// Normalise extracted text (see the module docs).
pub fn normalize(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut blank_lines = 0usize;
    let mut started = false;
    let unified = input.replace("\r\n", "\n").replace('\r', "\n");
    for line in unified.nfc().collect::<String>().split('\n') {
        let line = clean_line(line);
        if line.is_empty() {
            blank_lines += 1;
            continue;
        }
        if started {
            out.push_str(if blank_lines > 0 { "\n\n" } else { "\n" });
        }
        out.push_str(&line);
        started = true;
        blank_lines = 0;
    }
    out
}

/// Collapse whitespace runs to one space, drop control and zero-width characters,
/// trim both ends.
fn clean_line(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut space = false;
    for c in line.chars() {
        if c.is_whitespace() {
            space = true;
        } else if c.is_control() || matches!(c, '\u{200B}' | '\u{FEFF}' | '\u{00AD}') {
            // Drop: NUL, form feeds, BOMs, zero-width spaces, soft hyphens.
        } else {
            if space && !out.is_empty() {
                out.push(' ');
            }
            space = false;
            out.push(c);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collapses_whitespace_and_blank_lines() {
        let raw = "\u{FEFF}  Hello\t\t world  \r\n\r\n\r\n\nnext\u{0}line\rlast \n\n";
        assert_eq!(normalize(raw), "Hello world\n\nnextline\nlast");
    }

    #[test]
    fn composes_to_nfc() {
        // "e" + combining acute accent becomes one character.
        assert_eq!(normalize("cafe\u{301}"), "café");
        assert_eq!(normalize("cafe\u{301}").chars().count(), 4);
    }

    #[test]
    fn empty_and_whitespace_only() {
        assert_eq!(normalize(""), "");
        assert_eq!(normalize(" \n\t\n "), "");
    }
}
