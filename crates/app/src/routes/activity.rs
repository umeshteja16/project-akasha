//! `/api/v1/activity`: your activity timeline and security log.
//!
//! Account-level data, so only the browser session may read or clear it
//! ([`SessionUser`]); API tokens get 403. Events are only ever shown to their
//! owner. See `crate::activity` for what is recorded.

use axum::extract::State;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use super::cursor::{decode_cursor, encode_cursor};
use crate::{
    activity::{ActivityCategory, ActivityKind},
    auth::SessionUser,
    error::{ApiError, ErrorBody},
    extract::{Json, Query},
    state::AppState,
};
use akasha_core::Error;
use akasha_db::activity::{self, Event, ListFilter};

const DEFAULT_PAGE: i64 = 50;
const MAX_PAGE: i64 = 100;

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ActivityQuery {
    /// Comma-separated categories (`files,search,chat,collections,security`); default all.
    pub category: Option<String>,
    /// Comma-separated kinds (e.g. `auth.signed_in,auth.sign_in_failed`); default all.
    pub kind: Option<String>,
    /// Only events about this file.
    pub file_id: Option<Uuid>,
    /// `next_cursor` from the previous page.
    pub cursor: Option<String>,
    /// Page size, 1-100 (default 50).
    pub limit: Option<i64>,
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ClearQuery {
    /// Comma-separated categories to clear (default: everything except `security`,
    /// which is never cleared by hand).
    pub category: Option<String>,
}

/// Who acted: the browser session or an API token / MCP client.
#[derive(Debug, Clone, Copy, Serialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum ActivityVia {
    Session,
    Token,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ActivityItem {
    pub id: Uuid,
    pub kind: ActivityKind,
    pub category: ActivityCategory,
    pub created_at: DateTime<Utc>,
    /// What it was about, as named at the time: file name, search query,
    /// question, collection or token name.
    pub subject: Option<String>,
    /// The file, while it still exists.
    pub file_id: Option<Uuid>,
    /// The collection, while it still exists.
    pub collection_id: Option<Uuid>,
    /// The conversation, while it still exists.
    pub conversation_id: Option<Uuid>,
    /// Kind-specific facts, e.g. `{"from": "a.pdf", "to": "b.pdf"}` for a rename,
    /// `{"count": 3, "file_names": [..]}` for collection changes, `{"added": [..],
    /// "removed": [..]}` for tags, `{"results": 4, "mode": "hybrid"}` for searches.
    #[schema(value_type = Object)]
    pub details: serde_json::Value,
    pub via: Option<ActivityVia>,
    /// Client address (security events only).
    pub ip: Option<String>,
    /// Client user agent (security events only).
    pub user_agent: Option<String>,
}

impl ActivityItem {
    fn from_event(e: Event) -> Option<Self> {
        let kind = ActivityKind::parse(&e.kind)?;
        Some(Self {
            id: e.id,
            kind,
            category: kind.category(),
            created_at: e.created_at,
            subject: e.subject,
            file_id: e.file_id,
            collection_id: e.collection_id,
            conversation_id: e.conversation_id,
            details: e.details,
            via: match e.via.as_deref() {
                Some("session") => Some(ActivityVia::Session),
                Some("token") => Some(ActivityVia::Token),
                _ => None,
            },
            ip: e.ip,
            user_agent: e.user_agent,
        })
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ActivityPage {
    /// Newest first.
    pub items: Vec<ActivityItem>,
    /// Pass as `cursor` for older events; `null` on the last page.
    pub next_cursor: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ActivityCleared {
    pub deleted: u64,
}

fn split(raw: Option<&str>) -> Vec<&str> {
    raw.unwrap_or("")
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect()
}

fn parse_categories(raw: Option<&str>) -> Result<Vec<String>, Error> {
    split(raw)
        .into_iter()
        .map(|c| {
            serde_json::from_value::<ActivityCategory>(serde_json::Value::String(c.to_owned()))
                .map(|c| c.as_str().to_owned())
                .map_err(|_| Error::bad_request(format!("unknown category {c:?}")))
        })
        .collect()
}

/// Your activity, newest first: files uploaded, renamed, tagged, opened and
/// deleted, searches, questions, collection changes, and the security log
/// (sign-ins with address and browser, password and session changes, tokens,
/// rate limits). Kept for `AKASHA_ACTIVITY_RETENTION_DAYS` (default 365).
#[utoipa::path(
    get, path = "/api/v1/activity", tag = "activity", operation_id = "list_activity",
    params(ActivityQuery),
    responses(
        (status = 200, body = ActivityPage),
        (status = 400, body = ErrorBody), (status = 401, body = ErrorBody),
        (status = 403, body = ErrorBody),
    ),
    security(("session_cookie" = []))
)]
pub async fn list(
    State(state): State<AppState>,
    auth: SessionUser,
    Query(query): Query<ActivityQuery>,
) -> Result<Json<ActivityPage>, ApiError> {
    let limit = query.limit.unwrap_or(DEFAULT_PAGE);
    if !(1..=MAX_PAGE).contains(&limit) {
        return Err(Error::bad_request(format!("limit must be 1-{MAX_PAGE}")).into());
    }
    let kinds = split(query.kind.as_deref())
        .into_iter()
        .map(|k| {
            ActivityKind::parse(k)
                .map(|k| k.as_str().to_owned())
                .ok_or_else(|| Error::bad_request(format!("unknown kind {k:?}")))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let filter = ListFilter {
        categories: parse_categories(query.category.as_deref())?,
        kinds,
        file_id: query.file_id,
        before: query.cursor.as_deref().map(decode_cursor).transpose()?,
        limit: limit + 1,
    };
    let mut rows = activity::list(&state.db, auth.user_id, &filter).await?;
    let has_more = rows.len() as i64 > limit;
    rows.truncate(usize::try_from(limit).unwrap_or(0));
    let next_cursor = has_more
        .then(|| rows.last().map(|e| encode_cursor(e.created_at, e.id)))
        .flatten();
    Ok(Json(ActivityPage {
        items: rows
            .into_iter()
            .filter_map(ActivityItem::from_event)
            .collect(),
        next_cursor,
    }))
}

/// Clear your activity history (all of it, or some categories). The security
/// log is kept: it ages out with the retention period.
#[utoipa::path(
    delete, path = "/api/v1/activity", tag = "activity", operation_id = "clear_activity",
    params(ClearQuery),
    responses(
        (status = 200, body = ActivityCleared),
        (status = 400, body = ErrorBody), (status = 401, body = ErrorBody),
        (status = 403, body = ErrorBody),
    ),
    security(("session_cookie" = []))
)]
pub async fn clear(
    State(state): State<AppState>,
    auth: SessionUser,
    Query(query): Query<ClearQuery>,
) -> Result<Json<ActivityCleared>, ApiError> {
    let categories = parse_categories(query.category.as_deref())?;
    if categories.iter().any(|c| c == "security") {
        return Err(Error::bad_request("the security log cannot be cleared").into());
    }
    let deleted = activity::clear(&state.db, auth.user_id, &categories).await?;
    Ok(Json(ActivityCleared { deleted }))
}
