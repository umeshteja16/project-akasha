//! Query parameters of the search endpoints and their translation into an
//! [`akasha_search::SearchRequest`].

use akasha_search::{ChunkFilter, MAX_LIMIT, SearchMode, SearchRequest};
use chrono::{DateTime, Days, NaiveDate, Utc};
use serde::Deserialize;
use utoipa::IntoParams;
use uuid::Uuid;

use crate::routes::files::types::{FileCategory, normalize_tags};
use akasha_core::Error;

const DEFAULT_LIMIT: usize = 10;
const MAX_FILE_IDS: usize = 100;

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct SearchQuery {
    /// What to look for, 1-500 characters. Web-search syntax: `"exact phrase"`,
    /// `or`, and `-word` to exclude.
    pub q: String,
    /// `hybrid` (default), `keyword` or `semantic`.
    pub mode: Option<SearchMode>,
    /// Only files of this kind.
    #[serde(rename = "type")]
    #[param(rename = "type")]
    pub file_type: Option<FileCategory>,
    /// Uploaded on or after: a date (`2026-01-31`, UTC) or an RFC 3339 timestamp.
    pub from: Option<String>,
    /// Uploaded on or before this date (inclusive), or before this RFC 3339 timestamp.
    pub to: Option<String>,
    /// Comma-separated tags; files must carry all of them.
    pub tags: Option<String>,
    /// Only pinned (`true`) or unpinned (`false`) files.
    pub pinned: Option<bool>,
    /// Comma-separated file ids (up to 100) to search within.
    pub file_ids: Option<String>,
    /// Only files in this collection (404 if it is not yours).
    pub collection_id: Option<Uuid>,
    /// Results per page, 1-50 (default 10).
    pub limit: Option<usize>,
    /// 1-based page (default 1). Only the first 200 results can be paged through.
    pub page: Option<usize>,
    /// Reorder the top results with the cross-encoder, if one is configured (default true).
    pub rerank: Option<bool>,
    /// Also return loosely related results (found by meaning alone but below the
    /// relevance floor), marked `loosely_related` and listed after the matches
    /// (default false).
    pub include_weak: Option<bool>,
}

impl SearchQuery {
    pub fn into_request(self) -> Result<SearchRequest, Error> {
        let limit = self.limit.unwrap_or(DEFAULT_LIMIT);
        if !(1..=MAX_LIMIT).contains(&limit) {
            return Err(Error::bad_request(format!("limit must be 1-{MAX_LIMIT}")));
        }
        let page = self.page.unwrap_or(1);
        if page == 0 {
            return Err(Error::bad_request("page starts at 1"));
        }
        let offset = (page - 1)
            .checked_mul(limit)
            .ok_or_else(|| Error::bad_request("page is too large"))?;
        let tags = match self.tags.as_deref() {
            Some(raw) => normalize_tags(&split(raw))?,
            None => Vec::new(),
        };
        let file_ids = match self.file_ids.as_deref() {
            Some(raw) => parse_ids(raw)?,
            None => Vec::new(),
        };
        let filter = ChunkFilter {
            mime_patterns: self
                .file_type
                .map(FileCategory::mime_patterns)
                .unwrap_or_default(),
            from: self
                .from
                .as_deref()
                .map(|v| parse_time(v, false))
                .transpose()?,
            to: self
                .to
                .as_deref()
                .map(|v| parse_time(v, true))
                .transpose()?,
            tags,
            pinned: self.pinned,
            file_ids,
            exclude_file: None,
            collection_id: self.collection_id,
        };
        Ok(SearchRequest {
            query: self.q,
            mode: self.mode.unwrap_or_default(),
            filter,
            limit,
            offset,
            rerank: self.rerank.unwrap_or(true),
            include_weak: self.include_weak.unwrap_or(false),
        })
    }
}

fn split(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .collect()
}

fn parse_ids(raw: &str) -> Result<Vec<Uuid>, Error> {
    let ids = split(raw)
        .iter()
        .map(|s| s.parse::<Uuid>())
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| Error::bad_request("file_ids must be comma-separated UUIDs"))?;
    if ids.len() > MAX_FILE_IDS {
        return Err(Error::bad_request(format!(
            "at most {MAX_FILE_IDS} file ids"
        )));
    }
    Ok(ids)
}

/// A date means its first instant (or, as an inclusive upper bound, the next
/// day's first instant); a timestamp means itself.
fn parse_time(value: &str, end_of_day: bool) -> Result<DateTime<Utc>, Error> {
    if let Ok(ts) = DateTime::parse_from_rfc3339(value) {
        return Ok(ts.with_timezone(&Utc));
    }
    let date = NaiveDate::parse_from_str(value, "%Y-%m-%d").map_err(|_| {
        Error::bad_request("dates must look like 2026-01-31 or be RFC 3339 timestamps")
    })?;
    let date = if end_of_day {
        date.checked_add_days(Days::new(1))
            .ok_or_else(|| Error::bad_request("date out of range"))?
    } else {
        date
    };
    Ok(date.and_time(chrono::NaiveTime::MIN).and_utc())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn query(q: &str) -> SearchQuery {
        SearchQuery {
            q: q.into(),
            mode: None,
            file_type: None,
            from: None,
            to: None,
            tags: None,
            pinned: None,
            file_ids: None,
            collection_id: None,
            limit: None,
            page: None,
            rerank: None,
            include_weak: None,
        }
    }

    #[test]
    fn defaults_and_paging() {
        let req = query("x").into_request().expect("valid");
        assert_eq!((req.limit, req.offset, req.rerank), (10, 0, true));
        assert_eq!(req.mode, SearchMode::Hybrid);
        let mut q = query("x");
        q.page = Some(3);
        q.limit = Some(20);
        assert_eq!(q.into_request().expect("valid").offset, 40);
        let mut q = query("x");
        q.page = Some(0);
        assert!(q.into_request().is_err());
        let mut q = query("x");
        q.limit = Some(51);
        assert!(q.into_request().is_err());
    }

    #[test]
    fn dates_lists_and_types_are_parsed() {
        let mut q = query("x");
        q.from = Some("2026-01-31".into());
        q.to = Some("2026-02-01".into());
        q.tags = Some(" Work, q3 ,,".into());
        q.file_type = Some(FileCategory::Pdf);
        let id = Uuid::new_v4();
        q.file_ids = Some(format!("{id}"));
        let req = q.into_request().expect("valid");
        let f = req.filter;
        assert_eq!(
            f.from.expect("from").to_rfc3339(),
            "2026-01-31T00:00:00+00:00"
        );
        assert_eq!(f.to.expect("to").to_rfc3339(), "2026-02-02T00:00:00+00:00");
        assert_eq!(f.tags, vec!["work", "q3"]);
        assert_eq!(f.mime_patterns, vec!["application/pdf"]);
        assert_eq!(f.file_ids, vec![id]);

        let mut q = query("x");
        q.to = Some("2026-02-01T10:00:00+02:00".into());
        let to = q.into_request().expect("valid").filter.to.expect("to");
        assert_eq!(to.to_rfc3339(), "2026-02-01T08:00:00+00:00");

        for bad in ["yesterday", "2026-13-01"] {
            let mut q = query("x");
            q.from = Some(bad.into());
            assert!(q.into_request().is_err(), "{bad}");
        }
        let mut q = query("x");
        q.file_ids = Some("nope".into());
        assert!(q.into_request().is_err());
    }
}
