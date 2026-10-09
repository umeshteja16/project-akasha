//! Upload type detection: magic bytes (via `infer`) for binary formats, strict
//! UTF-8 for text formats (which have no magic bytes). Only an allow-list of types
//! is accepted; the declared file extension must agree with what the bytes are.

use akasha_core::Error;

/// How many leading bytes to collect before sniffing (every signature we accept
/// fits well inside this).
pub const SNIFF_LEN: usize = 8192;

/// Broad kind of a file, used for extension/content agreement and list filters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Category {
    Pdf,
    Image,
    /// Audio and video share containers (OGG, WebM, MP4), so they are not told apart
    /// when checking extensions.
    Media,
    Text,
}

/// Binary types we accept: (`infer` MIME, MIME we store, category).
const BINARY: &[(&str, &str, Category)] = &[
    ("application/pdf", "application/pdf", Category::Pdf),
    ("image/png", "image/png", Category::Image),
    ("image/jpeg", "image/jpeg", Category::Image),
    ("image/webp", "image/webp", Category::Image),
    ("image/gif", "image/gif", Category::Image),
    ("image/tiff", "image/tiff", Category::Image),
    ("audio/mpeg", "audio/mpeg", Category::Media),
    ("audio/x-wav", "audio/wav", Category::Media),
    ("audio/m4a", "audio/mp4", Category::Media),
    ("audio/ogg", "audio/ogg", Category::Media),
    ("audio/opus", "audio/ogg", Category::Media),
    ("audio/x-flac", "audio/flac", Category::Media),
    ("video/mp4", "video/mp4", Category::Media),
    ("video/webm", "video/webm", Category::Media),
    ("video/quicktime", "video/quicktime", Category::Media),
];

/// Extensions we know: (extension, category, MIME used for text types).
const EXTENSIONS: &[(&str, Category, &str)] = &[
    ("txt", Category::Text, "text/plain"),
    ("text", Category::Text, "text/plain"),
    ("md", Category::Text, "text/markdown"),
    ("markdown", Category::Text, "text/markdown"),
    ("csv", Category::Text, "text/csv"),
    ("json", Category::Text, "application/json"),
    ("pdf", Category::Pdf, ""),
    ("png", Category::Image, ""),
    ("jpg", Category::Image, ""),
    ("jpeg", Category::Image, ""),
    ("webp", Category::Image, ""),
    ("gif", Category::Image, ""),
    ("tif", Category::Image, ""),
    ("tiff", Category::Image, ""),
    ("mp3", Category::Media, ""),
    ("wav", Category::Media, ""),
    ("m4a", Category::Media, ""),
    ("ogg", Category::Media, ""),
    ("oga", Category::Media, ""),
    ("opus", Category::Media, ""),
    ("flac", Category::Media, ""),
    ("mp4", Category::Media, ""),
    ("m4v", Category::Media, ""),
    ("webm", Category::Media, ""),
    ("mov", Category::Media, ""),
];

/// The outcome of sniffing the first bytes of an upload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Detected {
    pub mime: &'static str,
    /// Text types must also pass [`TextValidator`] over the whole body.
    pub is_text: bool,
}

fn extension(name: &str) -> Option<String> {
    let (stem, ext) = name.rsplit_once('.')?;
    (!stem.is_empty() && !ext.is_empty()).then(|| ext.to_ascii_lowercase())
}

fn unsupported(what: &str) -> Error {
    Error::unsupported_media_type(format!(
        "{what}; accepted: PDF, PNG, JPEG, WebP, GIF, TIFF, text, Markdown, CSV, JSON, \
         MP3, WAV, M4A, OGG, FLAC, MP4, WebM, MOV"
    ))
}

/// Decide the type of an upload from its first bytes and its (sanitised) name.
pub fn detect(head: &[u8], name: &str) -> Result<Detected, Error> {
    let ext = extension(name);
    let declared = ext
        .as_deref()
        .map(|e| EXTENSIONS.iter().find(|(known, ..)| *known == e));

    if let Some(kind) = infer::get(head) {
        let Some(&(_, mime, category)) = BINARY.iter().find(|(m, ..)| *m == kind.mime_type())
        else {
            return Err(unsupported(&format!(
                "file type {} is not accepted",
                kind.mime_type()
            )));
        };
        if let Some(Some((ext, declared, _))) = declared
            && *declared != category
        {
            return Err(unsupported(&format!(
                "file contents ({mime}) do not match the .{ext} extension"
            )));
        }
        return Ok(Detected {
            mime,
            is_text: false,
        });
    }

    // No signature: only acceptable as text, and only under a text extension (or none).
    match declared {
        None => Ok(Detected {
            mime: "text/plain",
            is_text: true,
        }),
        Some(Some((_, Category::Text, mime))) => Ok(Detected {
            mime,
            is_text: true,
        }),
        Some(Some((ext, ..))) => Err(unsupported(&format!(
            "file contents do not match the .{ext} extension"
        ))),
        Some(None) => Err(unsupported(&format!(
            "the .{} extension is not accepted",
            ext.as_deref().unwrap_or_default()
        ))),
    }
}

/// Incremental check that a stream is UTF-8 text without NUL bytes. Handles
/// multi-byte characters split across chunks.
#[derive(Debug, Default)]
pub struct TextValidator {
    pending: Vec<u8>,
}

impl TextValidator {
    /// Feed the next chunk; `false` means the stream is not valid text.
    pub fn feed(&mut self, chunk: &[u8]) -> bool {
        if chunk.contains(&0) {
            return false;
        }
        let joined;
        let buf = if self.pending.is_empty() {
            chunk
        } else {
            joined = [self.pending.as_slice(), chunk].concat();
            joined.as_slice()
        };
        match std::str::from_utf8(buf) {
            Ok(_) => {
                self.pending.clear();
                true
            }
            // Incomplete character at the end: keep it for the next chunk.
            Err(err) if err.error_len().is_none() => {
                self.pending = buf[err.valid_up_to()..].to_vec();
                true
            }
            Err(_) => false,
        }
    }

    /// `true` if the stream ended on a character boundary.
    pub fn finish(self) -> bool {
        self.pending.is_empty()
    }
}

#[cfg(test)]
mod tests;
