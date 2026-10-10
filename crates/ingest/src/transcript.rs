//! Transcripts of audio and video: timestamped segments become one line each,
//! and every line's character span is kept with its time span so chunks (and
//! with them search results and citations) can say *when* something was said.

use crate::{Extraction, TimedSpan, normalize};

/// One recognised stretch of speech (milliseconds from the start).
#[derive(Debug, Clone, Copy)]
pub struct TimedText<'a> {
    pub start_ms: u32,
    pub end_ms: u32,
    pub text: &'a str,
}

/// Build the extraction of a recording from its segments (in time order).
/// Segments are joined with line breaks; empty ones are dropped. Text beyond
/// `max_chars` is cut (with a note).
pub fn transcript(segments: &[TimedText<'_>], max_chars: usize, notes: Vec<String>) -> Extraction {
    let mut text = String::new();
    let mut chars = 0usize;
    let mut spans = Vec::new();
    let mut notes = notes;
    for segment in segments {
        // One line per segment: collapse any line breaks inside it.
        let line = normalize(&segment.text.replace('\n', " "));
        if line.is_empty() {
            continue;
        }
        let separator = usize::from(!text.is_empty());
        let len = line.chars().count();
        if chars + separator + len > max_chars {
            notes.push(format!(
                "the transcript was truncated after {max_chars} characters"
            ));
            break;
        }
        if separator == 1 {
            text.push('\n');
        }
        let start = chars + separator;
        text.push_str(&line);
        chars = start + len;
        spans.push(TimedSpan {
            start_ms: segment.start_ms,
            end_ms: segment.end_ms.max(segment.start_ms),
            char_start: start,
            char_end: chars,
        });
    }
    if spans.is_empty() && !notes.iter().any(|n| n.contains("speech")) {
        notes.push("no speech was recognised in this recording".into());
    }
    Extraction {
        extractor: "transcript",
        text,
        pages: Vec::new(),
        segments: spans,
        notes,
    }
}

/// The time span covering characters `char_start..char_end` of a transcript:
/// from the start of the first segment it touches to the end of the last.
pub fn time_span(segments: &[TimedSpan], char_start: usize, char_end: usize) -> Option<(u32, u32)> {
    let first = segments.iter().find(|s| s.char_end > char_start)?;
    let last = segments
        .iter()
        .rev()
        .find(|s| s.char_start < char_end.max(char_start + 1))?;
    Some((first.start_ms, last.end_ms.max(first.start_ms)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seg(start_ms: u32, end_ms: u32, text: &str) -> TimedText<'_> {
        TimedText {
            start_ms,
            end_ms,
            text,
        }
    }

    #[test]
    fn lines_keep_their_times() {
        let e = transcript(
            &[
                seg(0, 1500, " Hello  there. "),
                seg(1500, 1600, "   "),
                seg(1600, 4000, "Général\nKenobi!"),
            ],
            1000,
            Vec::new(),
        );
        assert_eq!(e.extractor, "transcript");
        assert_eq!(e.text, "Hello there.\nGénéral Kenobi!");
        assert_eq!(e.segments.len(), 2);
        assert_eq!((e.segments[1].char_start, e.segments[1].char_end), (13, 28));
        assert_eq!(time_span(&e.segments, 0, 5), Some((0, 1500)));
        assert_eq!(time_span(&e.segments, 6, 20), Some((0, 4000)));
        assert_eq!(time_span(&e.segments, 13, 28), Some((1600, 4000)));
        assert_eq!(time_span(&e.segments, 40, 50), None);
        assert!(e.notes.is_empty());
    }

    #[test]
    fn truncates_and_notes_silence() {
        let e = transcript(&[seg(0, 1, "abcdef"), seg(1, 2, "ghij")], 8, Vec::new());
        assert_eq!(e.text, "abcdef");
        assert!(e.notes[0].contains("truncated"));
        let e = transcript(&[], 8, Vec::new());
        assert!(e.notes[0].contains("no speech"));
    }
}
