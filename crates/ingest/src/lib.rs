//! Text extraction and chunking (ADR 0008).
//!
//! A pure library: bytes and a MIME type in, normalised text with page hints and
//! chunks out. No database and no HTTP server; the only network access is the
//! optional one-time OCR model download in [`models`]. Everything here is CPU-bound
//! and blocking: call it from a blocking thread (`tokio::task::spawn_blocking`).
//!
//! - [`extract`] turns a file into an [`Extraction`]: the whole text plus, for PDFs,
//!   the character span of every page so search results can cite pages.
//! - [`chunk`] splits an extraction into overlapping [`Chunk`]s (never across a page
//!   boundary) with character offsets into [`Extraction::text`].
//!
//! Offsets are counted in Unicode scalar values (Rust `char`s, Postgres `text`
//! characters), not bytes and not UTF-16 units.

mod chunk;
mod error;
pub mod models;
mod normalize;
mod ocr;
mod pdf;
mod text;

use serde::{Deserialize, Serialize};

pub use chunk::{Chunk, ChunkOptions, chunk};
pub use error::IngestError;
pub use normalize::normalize;
pub use ocr::Ocr;

/// Bumped with this crate; stored with each extraction so stale ones can be found.
pub const EXTRACTOR_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Stop collecting text after this many characters by default (~10 MB of text).
pub const DEFAULT_MAX_CHARS: usize = 10_000_000;

/// How a file is extracted, decided from its (sniffed) MIME type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    PlainText,
    Markdown,
    Csv,
    Json,
    Pdf,
    Image,
    /// Audio and video: transcription is not implemented yet.
    Media,
    Unsupported,
}

impl Kind {
    pub fn of(mime: &str) -> Self {
        let mime = mime.split(';').next().unwrap_or_default().trim();
        match mime {
            "text/plain" => Self::PlainText,
            "text/markdown" => Self::Markdown,
            "text/csv" => Self::Csv,
            "application/json" => Self::Json,
            "application/pdf" => Self::Pdf,
            m if m.starts_with("image/") => Self::Image,
            m if m.starts_with("audio/") || m.starts_with("video/") => Self::Media,
            m if m.starts_with("text/") => Self::PlainText,
            _ => Self::Unsupported,
        }
    }

    /// Whether extraction may use an OCR engine (always for images; for PDFs only
    /// when a page has no text layer).
    pub fn may_need_ocr(self) -> bool {
        matches!(self, Self::Image | Self::Pdf)
    }
}

/// Where a page's text came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PageSource {
    /// The PDF's text layer.
    Text,
    /// Recognised from the page's scanned image.
    Ocr,
    /// No text layer, and OCR was disabled or could not read the page's image.
    NeedsOcr,
    /// The page could not be parsed (the rest of the document was).
    Unreadable,
}

/// One PDF page's span in [`Extraction::text`] (`char_start..char_end`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Page {
    /// 1-based page number.
    pub number: u32,
    pub char_start: usize,
    pub char_end: usize,
    pub source: PageSource,
}

/// The text of a file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Extraction {
    /// Which extractor produced it: `text`, `markdown`, `csv`, `json`, `pdf`, `ocr`
    /// or `none` (nothing extractable, e.g. audio).
    pub extractor: &'static str,
    /// Normalised text (NFC, `\n` line ends, collapsed blank runs). PDF pages are
    /// joined with a blank line.
    pub text: String,
    /// Page spans, in order; empty for formats without pages.
    pub pages: Vec<Page>,
    /// Human-readable remarks (OCR disabled, truncated, not supported yet, ...).
    pub notes: Vec<String>,
}

impl Extraction {
    /// Length of [`Self::text`] in characters.
    pub fn char_count(&self) -> usize {
        self.text.chars().count()
    }

    /// Pages that have no text and still want OCR.
    pub fn pages_needing_ocr(&self) -> usize {
        self.pages
            .iter()
            .filter(|p| p.source == PageSource::NeedsOcr)
            .count()
    }

    fn empty(extractor: &'static str, note: impl Into<String>) -> Self {
        Self {
            extractor,
            text: String::new(),
            pages: Vec::new(),
            notes: vec![note.into()],
        }
    }
}

/// Settings for [`extract`].
#[derive(Clone, Copy)]
pub struct Options<'a> {
    /// The OCR engine, if OCR is enabled and loaded. Without it, images and
    /// text-less PDF pages are marked [`PageSource::NeedsOcr`] instead.
    pub ocr: Option<&'a Ocr>,
    /// Text beyond this many characters is dropped (with a note).
    pub max_chars: usize,
}

impl Default for Options<'_> {
    fn default() -> Self {
        Self {
            ocr: None,
            max_chars: DEFAULT_MAX_CHARS,
        }
    }
}

/// Extract the text of a file. Never panics on malformed input: parser panics are
/// caught and reported as [`IngestError::Corrupt`].
pub fn extract(bytes: &[u8], mime: &str, options: &Options<'_>) -> Result<Extraction, IngestError> {
    match Kind::of(mime) {
        Kind::PlainText => Ok(text::plain(bytes, "text", options.max_chars)),
        Kind::Csv => Ok(text::plain(bytes, "csv", options.max_chars)),
        Kind::Json => Ok(text::plain(bytes, "json", options.max_chars)),
        Kind::Markdown => Ok(text::markdown(bytes, options.max_chars)),
        Kind::Pdf => pdf::extract(bytes, options),
        Kind::Image => match options.ocr {
            Some(ocr) => ocr::image(bytes, ocr, options.max_chars),
            None => Ok(Extraction::empty(
                "none",
                "OCR is disabled, so text in this image was not extracted",
            )),
        },
        Kind::Media => Ok(Extraction::empty(
            "none",
            "audio and video are not transcribed yet; the file is stored and downloadable",
        )),
        Kind::Unsupported => Ok(Extraction::empty(
            "none",
            format!("text extraction is not supported for {mime}"),
        )),
    }
}

/// Collects page texts into one string, tracking each page's character span.
pub(crate) struct TextBuilder {
    text: String,
    chars: usize,
    max_chars: usize,
    truncated: bool,
}

impl TextBuilder {
    pub(crate) fn new(max_chars: usize) -> Self {
        Self {
            text: String::new(),
            chars: 0,
            max_chars,
            truncated: false,
        }
    }

    /// Append a (normalised) segment, separated from the previous one by a blank
    /// line. Returns its `(char_start, char_end)`; empty segments take no space.
    pub(crate) fn push(&mut self, segment: &str) -> (usize, usize) {
        if segment.is_empty() || self.truncated {
            return (self.chars, self.chars);
        }
        if !self.text.is_empty() {
            self.text.push_str("\n\n");
            self.chars += 2;
        }
        let start = self.chars;
        let room = self.max_chars.saturating_sub(self.chars);
        let mut taken = 0;
        for c in segment.chars() {
            if taken == room {
                self.truncated = true;
                break;
            }
            self.text.push(c);
            taken += 1;
        }
        self.chars += taken;
        (start, self.chars)
    }

    pub(crate) fn is_full(&self) -> bool {
        self.truncated
    }

    /// The text, plus a note if it was cut short.
    pub(crate) fn finish(self, notes: &mut Vec<String>) -> String {
        if self.truncated {
            notes.push(format!(
                "text was truncated after {} characters",
                self.max_chars
            ));
        }
        self.text
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kinds_from_mime() {
        assert_eq!(Kind::of("text/plain; charset=utf-8"), Kind::PlainText);
        assert_eq!(Kind::of("text/markdown"), Kind::Markdown);
        assert_eq!(Kind::of("application/pdf"), Kind::Pdf);
        assert_eq!(Kind::of("image/png"), Kind::Image);
        assert_eq!(Kind::of("video/mp4"), Kind::Media);
        assert_eq!(Kind::of("application/zip"), Kind::Unsupported);
        assert!(Kind::Pdf.may_need_ocr() && !Kind::Csv.may_need_ocr());
    }

    #[test]
    fn builder_tracks_spans_and_truncates() {
        let mut b = TextBuilder::new(12);
        assert_eq!(b.push("héllo"), (0, 5));
        assert_eq!(b.push(""), (5, 5));
        assert_eq!(b.push("wörld!!"), (7, 12));
        assert!(b.is_full());
        let mut notes = Vec::new();
        let text = b.finish(&mut notes);
        assert_eq!(text, "héllo\n\nwörld");
        assert_eq!(notes.len(), 1);
    }

    #[test]
    fn media_and_disabled_ocr_are_empty_with_a_note() {
        let e = extract(b"\0\0", "audio/mpeg", &Options::default()).expect("media");
        assert_eq!((e.extractor, e.text.as_str()), ("none", ""));
        assert!(e.notes[0].contains("not transcribed"));
        let e = extract(b"\x89PNG", "image/png", &Options::default()).expect("image");
        assert!(e.notes[0].contains("OCR is disabled"));
    }
}
