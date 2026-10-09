//! Request and response bodies for `/api/v1/files`.

use akasha_db::files::{File, ListKey, ListOrder};
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
    /// Order of the list (default `newest`).
    pub sort: Option<FileSort>,
    /// `next_cursor` from the previous page (of the same `sort`).
    pub cursor: Option<String>,
    /// Page size, 1–200 (default 50).
    pub limit: Option<i64>,
}

/// Sort order of the file list.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum FileSort {
    /// Most recently uploaded first.
    #[default]
    Newest,
    Oldest,
    /// By name, A to Z, ignoring case.
    Name,
    /// Largest first.
    Size,
}

impl FileSort {
    pub fn order(self) -> ListOrder {
        match self {
            Self::Newest => ListOrder::Newest,
            Self::Oldest => ListOrder::Oldest,
            Self::Name => ListOrder::Name,
            Self::Size => ListOrder::Largest,
        }
    }

    fn tag(self) -> char {
        match self {
            Self::Newest => 'n',
            Self::Oldest => 'o',
            Self::Name => 'a',
            Self::Size => 's',
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct FileList {
    /// In the requested `sort` order.
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

/// Opaque keyset cursor: base64url of `<sort tag>:<id>:<key>`, where the key is
/// the creation time in µs, the size in bytes or the name (last, so it may hold `:`).
pub fn encode_cursor(sort: FileSort, file: &File) -> String {
    let key = match ListKey::of(file, sort.order()) {
        ListKey::Created(ts) => ts.timestamp_micros().to_string(),
        ListKey::Size(size) => size.to_string(),
        ListKey::Name(name) => name,
    };
    URL_SAFE_NO_PAD.encode(format!("{}:{}:{key}", sort.tag(), file.id))
}

/// Decode a cursor made by [`encode_cursor`] for the same `sort`.
pub fn decode_cursor(sort: FileSort, cursor: &str) -> Result<(ListKey, Uuid), Error> {
    let invalid = || Error::bad_request("invalid cursor");
    let raw = URL_SAFE_NO_PAD.decode(cursor).map_err(|_| invalid())?;
    let raw = String::from_utf8(raw).map_err(|_| invalid())?;
    let mut parts = raw.splitn(3, ':');
    let (Some(tag), Some(id), Some(key)) = (parts.next(), parts.next(), parts.next()) else {
        return Err(invalid());
    };
    if tag.chars().ne(std::iter::once(sort.tag())) {
        return Err(Error::bad_request(
            "cursor belongs to a different sort order",
        ));
    }
    let id: Uuid = id.parse().map_err(|_| invalid())?;
    let key = match sort {
        FileSort::Newest | FileSort::Oldest => {
            let micros: i64 = key.parse().map_err(|_| invalid())?;
            ListKey::Created(DateTime::from_timestamp_micros(micros).ok_or_else(invalid)?)
        }
        FileSort::Size => ListKey::Size(key.parse().map_err(|_| invalid())?),
        FileSort::Name => ListKey::Name(key.to_owned()),
    };
    Ok((key, id))
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

    fn file(name: &str, size: i64) -> File {
        let now = DateTime::from_timestamp_micros(1_760_000_000_123_456).expect("ts");
        File {
            id: Uuid::new_v4(),
            owner_id: Uuid::new_v4(),
            original_name: name.to_owned(),
            content_hash: String::new(),
            mime_type: "text/plain".into(),
            size_bytes: size,
            status: "ready".into(),
            error: None,
            is_pinned: false,
            tags: vec![],
            summary: None,
            auto_tags: vec![],
            enrichment_status: None,
            enrichment_model: None,
            enriched_from: None,
            enriched_at: None,
            created_at: now,
            updated_at: now,
        }
    }

    #[test]
    fn cursor_round_trips_per_sort_and_rejects_garbage() {
        let f = file("a:b.txt", 42);
        let cases = [
            (FileSort::Newest, ListKey::Created(f.created_at)),
            (FileSort::Oldest, ListKey::Created(f.created_at)),
            (FileSort::Name, ListKey::Name("a:b.txt".into())),
            (FileSort::Size, ListKey::Size(42)),
        ];
        for (sort, key) in cases {
            let cursor = encode_cursor(sort, &f);
            assert_eq!(decode_cursor(sort, &cursor).ok(), Some((key, f.id)));
        }
        let newest = encode_cursor(FileSort::Newest, &f);
        assert!(decode_cursor(FileSort::Size, &newest).is_err());
        let bad_id = URL_SAFE_NO_PAD.encode("n:not-a-uuid:1");
        for bad in ["", "!!", "bm9wZQ", &bad_id] {
            assert!(decode_cursor(FileSort::Newest, bad).is_err(), "{bad}");
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
