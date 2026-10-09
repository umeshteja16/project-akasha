//! Request and response bodies for `/api/v1/files`.

use akasha_db::files::File;
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use akasha_core::Error;

/// Processing state of a file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum FileStatus {
    /// Uploaded, waiting for the ingestion job.
    Pending,
    Processing,
    Ready,
    Failed,
}

impl FileStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Processing => "processing",
            Self::Ready => "ready",
            Self::Failed => "failed",
        }
    }

    fn from_db(value: &str) -> Self {
        match value {
            "pending" => Self::Pending,
            "processing" => Self::Processing,
            "ready" => Self::Ready,
            // The column has a CHECK constraint; anything else is a bug.
            _ => Self::Failed,
        }
    }
}

/// Broad file kind, for filtering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum FileCategory {
    Pdf,
    Image,
    Audio,
    Video,
    /// Plain text, Markdown, CSV and JSON.
    Text,
}

impl FileCategory {
    /// `LIKE` patterns on the stored MIME type.
    pub fn mime_patterns(self) -> Vec<String> {
        let patterns: &[&str] = match self {
            Self::Pdf => &["application/pdf"],
            Self::Image => &["image/%"],
            Self::Audio => &["audio/%"],
            Self::Video => &["video/%"],
            Self::Text => &["text/%", "application/json"],
        };
        patterns.iter().map(|p| (*p).to_owned()).collect()
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct FileResponse {
    pub id: Uuid,
    /// Sanitised original filename.
    pub name: String,
    /// Detected from the contents, not taken from the client.
    pub mime_type: String,
    pub size_bytes: i64,
    /// SHA-256 of the contents, lowercase hex.
    pub content_hash: String,
    pub status: FileStatus,
    /// Why processing failed, when `status` is `failed`.
    pub error: Option<String>,
    pub is_pinned: bool,
    /// The user's own tags (never changed by the model).
    pub tags: Vec<String>,
    /// Tags suggested by the language model, minus any already in `tags`.
    /// Filtering and searching by tag match both lists.
    pub auto_tags: Vec<String>,
    /// A short model-written description, once the file was enriched.
    pub summary: Option<String>,
    /// State of the model-written summary and tags; `null`: not enriched (yet).
    pub enrichment: Option<EnrichmentInfo>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// How the last enrichment (summary and suggested tags) went.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum EnrichmentStatus {
    Done,
    /// Nothing to describe (no extracted text).
    Skipped,
    /// The model failed or answered unusably; an earlier summary is kept.
    Failed,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct EnrichmentInfo {
    pub status: EnrichmentStatus,
    /// `provider/model` that wrote the summary and tags.
    pub model: Option<String>,
    pub updated_at: Option<DateTime<Utc>>,
}

impl EnrichmentInfo {
    fn from_db(f: &File) -> Option<Self> {
        let status = match f.enrichment_status.as_deref()? {
            "done" => EnrichmentStatus::Done,
            "skipped" => EnrichmentStatus::Skipped,
            _ => EnrichmentStatus::Failed,
        };
        Some(Self {
            status,
            model: f.enrichment_model.clone(),
            updated_at: f.enriched_at,
        })
    }
}

impl From<File> for FileResponse {
    fn from(f: File) -> Self {
        let enrichment = EnrichmentInfo::from_db(&f);
        let auto_tags = f
            .auto_tags
            .into_iter()
            .filter(|t| !f.tags.contains(t))
            .collect();
        Self {
            id: f.id,
            name: f.original_name,
            mime_type: f.mime_type,
            size_bytes: f.size_bytes,
            content_hash: f.content_hash,
            status: FileStatus::from_db(&f.status),
            error: f.error,
            is_pinned: f.is_pinned,
            tags: f.tags,
            auto_tags,
            summary: f.summary,
            enrichment,
            created_at: f.created_at,
            updated_at: f.updated_at,
        }
    }
}

/// `multipart/form-data` body of an upload.
#[derive(ToSchema)]
#[allow(dead_code)] // documentation only; the handler reads the stream directly
pub struct UploadForm {
    /// The file. Its type is detected from the bytes; the filename is sanitised.
    #[schema(value_type = String, format = Binary)]
    pub file: Vec<u8>,
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ListQuery {
    pub status: Option<FileStatus>,
    pub pinned: Option<bool>,
    /// Only files carrying this tag (their own or a suggested one).
    pub tag: Option<String>,
    pub category: Option<FileCategory>,
    /// `next_cursor` from the previous page.
    pub cursor: Option<String>,
    /// Page size, 1–200 (default 50).
    pub limit: Option<i64>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct FileList {
    /// Newest first.
    pub items: Vec<FileResponse>,
    /// Pass as `cursor` to get the next page; `null` on the last page.
    pub next_cursor: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateFileRequest {
    /// New display name (sanitised like uploads).
    pub name: Option<String>,
    pub is_pinned: Option<bool>,
    /// Replaces all tags. Trimmed and lowercased; at most 32, each 1–50 characters.
    pub tags: Option<Vec<String>>,
    /// Replaces the model-suggested tags (e.g. to drop a wrong one); same rules
    /// as `tags`. Re-running enrichment replaces them again; to keep a
    /// suggestion for good, add it to `tags`.
    pub auto_tags: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct BulkDeleteRequest {
    /// Up to 100 file ids. Ids that are unknown (or not yours) are ignored.
    pub ids: Vec<Uuid>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct BulkDeleteResponse {
    /// The ids that were deleted.
    pub deleted: Vec<Uuid>,
}

/// Opaque keyset cursor: base64url of `<created_at µs>:<id>`.
pub fn encode_cursor(created_at: DateTime<Utc>, id: Uuid) -> String {
    URL_SAFE_NO_PAD.encode(format!("{}:{id}", created_at.timestamp_micros()))
}

pub fn decode_cursor(cursor: &str) -> Result<(DateTime<Utc>, Uuid), Error> {
    let invalid = || Error::bad_request("invalid cursor");
    let raw = URL_SAFE_NO_PAD.decode(cursor).map_err(|_| invalid())?;
    let raw = String::from_utf8(raw).map_err(|_| invalid())?;
    let (micros, id) = raw.split_once(':').ok_or_else(invalid)?;
    let micros: i64 = micros.parse().map_err(|_| invalid())?;
    let created_at = DateTime::from_timestamp_micros(micros).ok_or_else(invalid)?;
    let id = id.parse().map_err(|_| invalid())?;
    Ok((created_at, id))
}

const MAX_TAGS: usize = 32;
const MAX_TAG_CHARS: usize = 50;

/// Trim, lowercase and de-duplicate tags (keeping first-seen order).
pub fn normalize_tags(raw: &[String]) -> Result<Vec<String>, Error> {
    let mut tags: Vec<String> = Vec::with_capacity(raw.len());
    for tag in raw {
        let tag = tag.trim().to_lowercase();
        if tag.is_empty() || tag.chars().count() > MAX_TAG_CHARS {
            return Err(Error::bad_request(format!(
                "tags must be 1-{MAX_TAG_CHARS} characters"
            )));
        }
        if tag.chars().any(char::is_control) {
            return Err(Error::bad_request(
                "tags must not contain control characters",
            ));
        }
        if !tags.contains(&tag) {
            tags.push(tag);
        }
    }
    if tags.len() > MAX_TAGS {
        return Err(Error::bad_request(format!("at most {MAX_TAGS} tags")));
    }
    Ok(tags)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cursor_round_trips_and_rejects_garbage() {
        let now = DateTime::from_timestamp_micros(1_760_000_000_123_456).expect("ts");
        let id = Uuid::new_v4();
        assert_eq!(decode_cursor(&encode_cursor(now, id)).ok(), Some((now, id)));
        for bad in ["", "!!", "bm9wZQ", &URL_SAFE_NO_PAD.encode("1:not-a-uuid")] {
            assert!(decode_cursor(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn tags_are_normalised_and_bounded() {
        let raw = vec![" Work ".to_owned(), "work".to_owned(), "Q3".to_owned()];
        assert_eq!(
            normalize_tags(&raw).ok(),
            Some(vec!["work".into(), "q3".into()])
        );
        assert!(normalize_tags(&[" ".to_owned()]).is_err());
        assert!(normalize_tags(&["x".repeat(51)]).is_err());
        assert!(normalize_tags(&["a\nb".to_owned()]).is_err());
        let many: Vec<String> = (0..33).map(|i| i.to_string()).collect();
        assert!(normalize_tags(&many).is_err());
    }
}
