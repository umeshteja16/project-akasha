//! Sources given to the model and the `[n]` citations in its answer.

use akasha_search::ChunkHit;
use serde::Serialize;
use utoipa::ToSchema;
use uuid::Uuid;

/// Characters of a passage kept as its quote.
const QUOTE_CHARS: usize = 300;

/// A numbered passage from the user's files: shown to the model as `[n]`, and
/// stored with an answer that cites it.
#[derive(Debug, Clone, PartialEq, Serialize, serde::Deserialize, ToSchema)]
pub struct Citation {
    /// The number the answer uses (`[n]`).
    pub n: u32,
    pub chunk_id: i64,
    pub file_id: Uuid,
    pub file_name: String,
    /// 1-based PDF page; `null` for formats without pages.
    pub page: Option<i32>,
    /// Audio and video: when the passage's speech starts (milliseconds); `null`
    /// for other formats.
    #[serde(default)]
    pub start_ms: Option<i32>,
    /// Audio and video: when the passage's speech ends (milliseconds).
    #[serde(default)]
    pub end_ms: Option<i32>,
    /// `[char_start, char_end)` of the passage in the file's extracted text (characters).
    pub char_start: i32,
    pub char_end: i32,
    /// The start of the passage.
    pub quote: String,
}

/// A source: its citation and the full passage text for the prompt.
#[derive(Debug, Clone)]
pub struct Source {
    pub citation: Citation,
    pub text: String,
}

impl Source {
    pub fn new(n: u32, hit: &ChunkHit) -> Self {
        let quote = quote(&hit.text);
        Self {
            citation: Citation {
                n,
                chunk_id: hit.chunk.chunk_id,
                file_id: hit.file.id,
                file_name: hit.file.name.clone(),
                page: hit.chunk.page,
                start_ms: hit.chunk.start_ms,
                end_ms: hit.chunk.end_ms,
                char_start: hit.chunk.char_start,
                char_end: hit.chunk.char_end,
                quote,
            },
            text: hit.text.clone(),
        }
    }
}

/// `ms` as a clock time: `4:05`, or `1:02:03` past an hour.
pub fn timestamp(ms: i32) -> String {
    let total = ms.max(0) / 1000;
    let (h, m, s) = (total / 3600, total / 60 % 60, total % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

fn quote(text: &str) -> String {
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if text.chars().count() <= QUOTE_CHARS {
        return text;
    }
    let mut q: String = text.chars().take(QUOTE_CHARS).collect();
    q.push('…');
    q
}

/// The sources `answer` cites, in order of first citation. Markers are `[n]`
/// or lists like `[1, 3]`; numbers that match no source are dropped.
pub fn cited(answer: &str, sources: &[Source]) -> Vec<Citation> {
    let mut out: Vec<Citation> = Vec::new();
    for n in markers(answer) {
        if out.iter().any(|c| c.n == n) {
            continue;
        }
        if let Some(s) = sources.iter().find(|s| s.citation.n == n) {
            out.push(s.citation.clone());
        }
    }
    out
}

/// Every number inside `[...]` groups made only of digits, commas and spaces.
fn markers(text: &str) -> Vec<u32> {
    let mut found = Vec::new();
    let mut rest = text;
    while let Some(open) = rest.find('[') {
        rest = &rest[open + 1..];
        let Some(close) = rest.find(']') else { break };
        let inner = &rest[..close];
        let valid = !inner.trim().is_empty()
            && inner.len() <= 40
            && inner
                .chars()
                .all(|c| c.is_ascii_digit() || c == ',' || c == ' ');
        if valid {
            found.extend(
                inner
                    .split(',')
                    .filter_map(|n| n.trim().parse::<u32>().ok()),
            );
            rest = &rest[close + 1..];
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source(n: u32) -> Source {
        Source {
            citation: Citation {
                n,
                chunk_id: i64::from(n) * 10,
                file_id: Uuid::nil(),
                file_name: format!("f{n}.txt"),
                page: None,
                start_ms: None,
                end_ms: None,
                char_start: 0,
                char_end: 5,
                quote: "q".into(),
            },
            text: "t".into(),
        }
    }

    #[test]
    fn timestamps_read_like_clocks() {
        assert_eq!(timestamp(0), "0:00");
        assert_eq!(timestamp(65_400), "1:05");
        assert_eq!(timestamp(3_723_000), "1:02:03");
        assert_eq!(timestamp(-5), "0:00");
    }

    #[test]
    fn markers_cover_common_shapes_and_skip_other_brackets() {
        assert_eq!(
            markers("a [1] b [2][3] c [4, 5] d [6,7]"),
            [1, 2, 3, 4, 5, 6, 7]
        );
        assert_eq!(markers("[link](x) [x] [] [ ] [1a] arr[0] [[2]]"), [0, 2]);
        assert_eq!(markers("unterminated [3"), Vec::<u32>::new());
    }

    #[test]
    fn citations_keep_first_order_dedupe_and_drop_unknown_sources() {
        let sources = [source(1), source(2), source(3)];
        let cites = cited("B [2]. A [1][2]. Bogus [9] and [0].", &sources);
        let ns: Vec<u32> = cites.iter().map(|c| c.n).collect();
        assert_eq!(ns, [2, 1]);
        assert_eq!(cites[0].chunk_id, 20);
        assert!(cited("no citations", &sources).is_empty());
    }

    #[test]
    fn quotes_are_whitespace_normalised_and_bounded() {
        assert_eq!(quote("a\n\n b\tc"), "a b c");
        let long = quote(&"word ".repeat(200));
        assert_eq!(long.chars().count(), QUOTE_CHARS + 1);
        assert!(long.ends_with('…'));
    }
}
