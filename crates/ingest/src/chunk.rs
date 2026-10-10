//! Splitting an extraction into overlapping chunks for indexing and citations.
//!
//! Sizes are in characters. The default (~2000 characters, ~250 overlap) is about
//! 512 tokens of English text with ~12% overlap; the embeddings step may switch to
//! a tokenizer-based size once its model is chosen. `text-splitter` prefers
//! semantic boundaries (paragraphs, then sentences, then words).

use text_splitter::{ChunkConfig, TextSplitter};

use crate::Extraction;

/// Chunk sizes, in characters.
#[derive(Debug, Clone, Copy)]
pub struct ChunkOptions {
    /// Largest chunk. Chunks aim for at least three quarters of this.
    pub max_chars: usize,
    /// Characters shared by neighbouring chunks of the same page.
    pub overlap_chars: usize,
}

impl Default for ChunkOptions {
    fn default() -> Self {
        Self {
            max_chars: 2000,
            overlap_chars: 250,
        }
    }
}

/// A piece of an extraction's text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chunk {
    /// 0-based position in the file.
    pub index: u32,
    /// The PDF page it comes from; `None` for formats without pages.
    pub page: Option<u32>,
    /// For transcripts: when the chunk's speech starts and ends (milliseconds).
    pub start_ms: Option<u32>,
    pub end_ms: Option<u32>,
    /// `text == extraction.text.chars().skip(char_start).take(char_end - char_start)`.
    pub char_start: usize,
    pub char_end: usize,
    pub text: String,
}

/// Split `extraction` into chunks. Chunks never span two pages.
pub fn chunk(extraction: &Extraction, options: &ChunkOptions) -> Vec<Chunk> {
    let max = options.max_chars.max(1);
    let range = (max * 3 / 4).max(1)..max;
    let config = ChunkConfig::new(range.clone())
        .with_overlap(options.overlap_chars)
        .unwrap_or_else(|_| ChunkConfig::new(range));
    let splitter = TextSplitter::new(config);

    let text = extraction.text.as_str();
    let segments: Vec<(Option<u32>, usize, usize)> = if extraction.pages.is_empty() {
        vec![(None, 0, extraction.char_count())]
    } else {
        extraction
            .pages
            .iter()
            .map(|p| (Some(p.number), p.char_start, p.char_end))
            .collect()
    };

    let mut chunks = Vec::new();
    let mut cursor = CharCursor::new(text);
    for (page, start, end) in segments {
        if end <= start {
            continue;
        }
        let from = cursor.byte_at(start);
        let to = cursor.byte_at(end);
        for piece in splitter.chunk_char_indices(&text[from..to]) {
            let char_start = start + piece.char_offset;
            let char_end = char_start + piece.chunk.chars().count();
            let times = crate::time_span(&extraction.segments, char_start, char_end);
            chunks.push(Chunk {
                index: u32::try_from(chunks.len()).unwrap_or(u32::MAX),
                page,
                start_ms: times.map(|t| t.0),
                end_ms: times.map(|t| t.1),
                char_start,
                char_end,
                text: piece.chunk.to_owned(),
            });
        }
    }
    chunks
}

/// Converts increasing character offsets to byte offsets in one pass.
struct CharCursor<'a> {
    text: &'a str,
    chars: usize,
    bytes: usize,
}

impl<'a> CharCursor<'a> {
    fn new(text: &'a str) -> Self {
        Self {
            text,
            chars: 0,
            bytes: 0,
        }
    }

    /// Byte offset of character `target` (clamped to the end). `target` must not
    /// be smaller than the previous call's.
    fn byte_at(&mut self, target: usize) -> usize {
        let mut iter = self.text[self.bytes..].char_indices();
        while self.chars < target {
            match iter.next() {
                Some((_, c)) => {
                    self.bytes += c.len_utf8();
                    self.chars += 1;
                }
                None => break,
            }
        }
        self.bytes
    }
}
