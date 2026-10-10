//! Read-only tools: search, get_file, read_file, list_files.

use akasha_db::{extraction, files};
use serde::Serialize;
use serde_json::json;
use uuid::Uuid;

use super::{
    Caller,
    output::{ToolError, ToolResult, clip},
    tools::{FileArgs, Kind, ListArgs, ReadArgs, SearchArgs},
};
use crate::{
    rate_limit,
    routes::{
        files::{
            extraction::PageSpan,
            types::{FileCategory, FileResponse, FileSort, decode_cursor, encode_cursor},
        },
        search::{models, params::SearchQuery},
    },
    state::AppState,
};

const SEARCH_DEFAULT: usize = 8;
const SEARCH_MAX: usize = 20;
/// Characters of each passage returned by `search`.
const PASSAGE_CHARS: usize = 1_200;
const READ_DEFAULT: u32 = 8_000;
const READ_MIN: u32 = 100;
const READ_MAX: u32 = 20_000;
const LIST_DEFAULT: i64 = 20;
const LIST_MAX: i64 = 50;
const SUMMARY_CHARS: usize = 300;

impl From<Kind> for FileCategory {
    fn from(k: Kind) -> Self {
        match k {
            Kind::Pdf => Self::Pdf,
            Kind::Image => Self::Image,
            Kind::Audio => Self::Audio,
            Kind::Video => Self::Video,
            Kind::Text => Self::Text,
        }
    }
}

fn enum_arg<T: serde::de::DeserializeOwned>(
    name: &str,
    value: Option<String>,
) -> Result<Option<T>, ToolError> {
    value
        .map(|v| {
            serde_json::from_value(serde_json::Value::String(v.clone()))
                .map_err(|_| ToolError::new(format!("unknown {name} `{v}`")))
        })
        .transpose()
}

fn join<T: ToString>(items: Option<Vec<T>>) -> Option<String> {
    items.map(|v| {
        v.iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(",")
    })
}

#[derive(Serialize)]
struct Passage {
    file_id: Uuid,
    file_name: String,
    page: Option<i32>,
    /// Audio and video: when the passage is said (`m:ss`).
    #[serde(skip_serializing_if = "Option::is_none")]
    at: Option<String>,
    chunk_id: i64,
    char_start: i32,
    char_end: i32,
    loosely_related: bool,
    text: String,
}

pub async fn search(state: &AppState, caller: Caller, a: SearchArgs) -> ToolResult {
    rate_limit::check_user_audited(&state.db, &state.search_limiter, caller.actor(), "searches")?;
    let limit = a.limit.unwrap_or(SEARCH_DEFAULT);
    if !(1..=SEARCH_MAX).contains(&limit) {
        return Err(ToolError::new(format!("limit must be 1-{SEARCH_MAX}")));
    }
    let query = SearchQuery {
        q: a.query,
        mode: enum_arg("mode", a.mode)?,
        file_type: a.kind.map(Into::into),
        from: a.from,
        to: a.to,
        tags: a.tags.map(|t| t.join(",")),
        pinned: None,
        file_ids: join(a.file_ids),
        collection_id: super::collections::resolve_opt(state, caller, a.collection.as_deref())
            .await?,
        limit: Some(limit),
        page: None,
        rerank: None,
        include_weak: a.include_weak,
    };
    let req = query.into_request()?;
    let models = models(state).await;
    let res = akasha_search::search_chunks(&state.db, caller.user_id, &req, &models).await?;
    let passages: Vec<Passage> = res
        .results
        .into_iter()
        .map(|hit| Passage {
            file_id: hit.file.id,
            file_name: hit.file.name,
            page: hit.chunk.page,
            at: hit.chunk.start_ms.map(crate::chat::citations::timestamp),
            chunk_id: hit.chunk.chunk_id,
            char_start: hit.chunk.char_start,
            char_end: hit.chunk.char_end,
            loosely_related: hit.chunk.loosely_related,
            text: clip(hit.text.trim(), PASSAGE_CHARS),
        })
        .collect();
    let mut out = json!({
        "query": res.meta.query,
        "mode": res.meta.mode,
        "results": passages,
    });
    if res.meta.degraded || !res.meta.warnings.is_empty() {
        out["warnings"] = json!(res.meta.warnings);
    }
    if let Some(s) = res.meta.suggestion {
        out["did_you_mean"] = json!(s);
    }
    if res.meta.loosely_related > 0 {
        out["loosely_related_hidden"] = json!(res.meta.loosely_related);
    }
    if passages_empty(&out) {
        out["note"] = json!("Nothing in the library matched. Try other words or mode `keyword`.");
    }
    Ok(out)
}

fn passages_empty(out: &serde_json::Value) -> bool {
    out["results"].as_array().is_some_and(Vec::is_empty)
}

async fn owned_file(state: &AppState, caller: Caller, id: Uuid) -> Result<files::File, ToolError> {
    files::get(&state.db, caller.user_id, id)
        .await?
        .ok_or_else(|| ToolError::new(format!("no file with id {id} in this library")))
}

pub async fn get_file(state: &AppState, caller: Caller, a: FileArgs) -> ToolResult {
    let file = owned_file(state, caller, a.file_id).await?;
    let ext = extraction::get(&state.db, caller.user_id, file.id, 0, 0).await?;
    let mut out = serde_json::to_value(FileResponse::from(file))?;
    if let Some(obj) = out.as_object_mut() {
        obj.remove("content_hash");
        obj.insert(
            "text".into(),
            match ext {
                Some(e) => json!({
                    "char_count": e.char_count,
                    "page_count": e.page_count,
                    "chunk_count": e.chunk_count,
                    "notes": e.notes,
                }),
                None => json!(null),
            },
        );
    }
    Ok(out)
}

pub async fn read_file(state: &AppState, caller: Caller, a: ReadArgs) -> ToolResult {
    let max = a.max_chars.unwrap_or(READ_DEFAULT);
    if !(READ_MIN..=READ_MAX).contains(&max) {
        return Err(ToolError::new(format!(
            "max_chars must be {READ_MIN}-{READ_MAX}"
        )));
    }
    let file = owned_file(state, caller, a.file_id).await?;
    let Some(head) = extraction::get(&state.db, caller.user_id, file.id, 0, 0).await? else {
        return Err(ToolError::new(format!(
            "this file has no extracted text yet (status: {})",
            file.status
        )));
    };
    let pages: Vec<PageSpan> = serde_json::from_value(head.pages).unwrap_or_default();
    let (start, end) = match a.page {
        Some(n) => {
            let span = pages.iter().find(|p| p.number == n).ok_or_else(|| {
                ToolError::new(match head.page_count {
                    Some(count) => format!("page must be 1-{count}"),
                    None => "this file has no pages; use offset".to_owned(),
                })
            })?;
            let start = i64::try_from(span.char_start).unwrap_or(i64::MAX);
            let len = i64::try_from(span.char_end.saturating_sub(span.char_start)).unwrap_or(0);
            (start, start + len.min(i64::from(max)))
        }
        None => {
            let start = i64::from(a.offset.unwrap_or(0));
            (start, start + i64::from(max))
        }
    };
    let total = i64::from(head.char_count);
    if start >= total && total > 0 {
        return Err(ToolError::new(format!(
            "offset is past the end ({total} characters)"
        )));
    }
    let offset = i32::try_from(start).map_err(|_| ToolError::new("offset too large"))?;
    let limit = i32::try_from(end - start).unwrap_or(i32::MAX);
    let window = extraction::get(&state.db, caller.user_id, file.id, offset, limit)
        .await?
        .ok_or_else(|| ToolError::new("this file has no extracted text"))?;
    let end = (start + i64::try_from(window.text.chars().count()).unwrap_or(0)).min(total);
    let in_window: Vec<u32> = pages
        .iter()
        .filter(|p| (p.char_start as i64) < end && (p.char_end as i64) > start)
        .map(|p| p.number)
        .collect();
    Ok(json!({
        "file_id": file.id,
        "file_name": file.original_name,
        "char_count": total,
        "page_count": head.page_count,
        "offset": start,
        "end": end,
        "pages": in_window,
        "next_offset": (end < total).then_some(end),
        "text": window.text,
    }))
}

pub async fn list_files(state: &AppState, caller: Caller, a: ListArgs) -> ToolResult {
    let limit = a.limit.unwrap_or(LIST_DEFAULT);
    if !(1..=LIST_MAX).contains(&limit) {
        return Err(ToolError::new(format!("limit must be 1-{LIST_MAX}")));
    }
    let sort: FileSort = enum_arg("sort", a.sort)?.unwrap_or_default();
    let filter = files::ListFilter {
        status: None,
        pinned: a.pinned,
        tag: a.tag.map(|t| t.trim().to_lowercase()),
        mime_patterns: a
            .kind
            .map(|k| FileCategory::from(k).mime_patterns())
            .unwrap_or_default(),
        collection_id: super::collections::resolve_opt(state, caller, a.collection.as_deref())
            .await?,
        order: sort.order(),
        after: a
            .cursor
            .as_deref()
            .map(|c| decode_cursor(sort, c))
            .transpose()?,
        limit: limit + 1,
    };
    let mut rows = files::list(&state.db, caller.user_id, &filter).await?;
    let has_more = rows.len() as i64 > limit;
    rows.truncate(usize::try_from(limit).unwrap_or(0));
    let next_cursor = has_more
        .then(|| rows.last().map(|f| encode_cursor(sort, f)))
        .flatten();
    let items: Vec<_> = rows
        .into_iter()
        .map(|f| {
            json!({
                "file_id": f.id,
                "name": f.original_name,
                "mime_type": f.mime_type,
                "size_bytes": f.size_bytes,
                "status": f.status,
                "tags": f.tags,
                "summary": f.summary.as_deref().map(|s| clip(s, SUMMARY_CHARS)),
                "created_at": f.created_at,
                "last_opened_at": f.last_opened_at,
            })
        })
        .collect();
    Ok(json!({ "files": items, "next_cursor": next_cursor }))
}
