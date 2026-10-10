//! Request and response bodies for `/api/v1/sources`.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use akasha_core::Error;
use akasha_db::sources::SourceSummary;

use crate::sources::{filter, scan::ScanStats};

pub const NAME_MAX: usize = 100;
/// Watched folders per user.
pub const MAX_SOURCES: usize = 20;

/// What happens to an imported file when it disappears from the folder.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum SourceOnDelete {
    /// Delete it from Akasha too.
    Delete,
    /// Keep it in Akasha (no longer linked to the folder).
    Keep,
}

impl SourceOnDelete {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Delete => "delete",
            Self::Keep => "keep",
        }
    }
}

/// Sync state of a source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum SourceStatus {
    /// Added, first scan not finished yet.
    Pending,
    Scanning,
    /// Last scan finished.
    Ok,
    /// Last scan stopped: see `last_error`.
    Error,
}

/// Counts from the last finished scan.
#[derive(Debug, Clone, Copy, Default, Serialize, ToSchema)]
pub struct ScanCounts {
    /// Matching files in the folder.
    pub files: u64,
    pub imported: u64,
    pub updated: u64,
    pub removed: u64,
    pub skipped: u64,
}

/// A watched folder.
#[derive(Debug, Serialize, ToSchema)]
pub struct SourceResponse {
    pub id: Uuid,
    /// Always `folder` for now.
    pub kind: String,
    pub name: String,
    /// Absolute path on the server.
    pub path: String,
    pub include_globs: Vec<String>,
    pub exclude_globs: Vec<String>,
    pub on_delete: SourceOnDelete,
    /// Markdown front-matter `tags:` become file tags.
    pub import_tags: bool,
    /// `false`: paused.
    pub enabled: bool,
    pub status: SourceStatus,
    pub last_error: Option<String>,
    pub last_scan_at: Option<DateTime<Utc>>,
    pub last_scan: ScanCounts,
    /// Files from this folder in your library.
    pub file_count: i64,
    /// Files not imported (unsupported content, too large, quota).
    pub skipped_count: i64,
    pub created_at: DateTime<Utc>,
}

impl From<SourceSummary> for SourceResponse {
    fn from(s: SourceSummary) -> Self {
        let src = s.source;
        let stats: ScanStats = serde_json::from_value(src.last_scan).unwrap_or_default();
        Self {
            id: src.id,
            kind: src.kind,
            name: src.name,
            path: src.path,
            include_globs: src.include_globs,
            exclude_globs: src.exclude_globs,
            on_delete: if src.on_delete == "keep" {
                SourceOnDelete::Keep
            } else {
                SourceOnDelete::Delete
            },
            import_tags: src.import_tags,
            enabled: src.enabled,
            status: match src.status.as_str() {
                "scanning" => SourceStatus::Scanning,
                "ok" => SourceStatus::Ok,
                "error" => SourceStatus::Error,
                _ => SourceStatus::Pending,
            },
            last_error: src.last_error,
            last_scan_at: src.last_scan_at,
            last_scan: ScanCounts {
                files: stats.files,
                imported: stats.imported,
                updated: stats.updated,
                removed: stats.removed,
                skipped: stats.skipped,
            },
            file_count: s.file_count,
            skipped_count: s.skipped_count,
            created_at: src.created_at,
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct SourceList {
    pub items: Vec<SourceResponse>,
    /// Folders you may add sources under (empty: watched folders are off).
    pub roots: Vec<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CreateSource {
    /// Absolute path of a folder inside one of `roots`.
    pub path: String,
    /// Default: the folder's name.
    pub name: Option<String>,
    /// Globs on the path inside the folder (`**/*.md`); none = every supported file.
    pub include_globs: Option<Vec<String>>,
    /// Globs to leave out (`Archive/**`). Hidden files and folders (`.obsidian/`,
    /// `.trash/`) are always left out.
    pub exclude_globs: Option<Vec<String>>,
    /// Default `delete`.
    pub on_delete: Option<SourceOnDelete>,
    /// Default `true`.
    pub import_tags: Option<bool>,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct UpdateSource {
    pub name: Option<String>,
    pub include_globs: Option<Vec<String>>,
    pub exclude_globs: Option<Vec<String>>,
    pub on_delete: Option<SourceOnDelete>,
    pub import_tags: Option<bool>,
    /// `false` pauses syncing, `true` resumes it (and scans).
    pub enabled: Option<bool>,
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct DeleteSourceQuery {
    /// Also delete the files this folder added to your library (default `false`:
    /// they stay).
    #[serde(default)]
    pub delete_files: bool,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct SourceRemoved {
    /// Files deleted along with the source.
    pub deleted_files: usize,
}

pub fn clean_name(name: &str) -> Result<String, Error> {
    let name = name.trim();
    if name.is_empty() || name.chars().count() > NAME_MAX {
        return Err(Error::bad_request(format!(
            "name must be 1-{NAME_MAX} characters"
        )));
    }
    Ok(name.to_owned())
}

pub fn clean_globs(globs: &[String]) -> Result<Vec<String>, Error> {
    filter::clean_globs(globs).map_err(Error::bad_request)
}
