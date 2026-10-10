//! Collections for agents: the `list_collections` tool and resolving a
//! `collection` argument (a name or an id) to one of the caller's collections.

use serde_json::json;
use uuid::Uuid;

use super::{
    Caller,
    output::{ToolError, ToolResult},
};
use crate::state::AppState;
use akasha_db::collections;

const DESCRIPTION_CHARS: usize = 300;

/// The caller's collection named `raw` (any case) or with that id.
pub async fn resolve(state: &AppState, caller: Caller, raw: &str) -> Result<Uuid, ToolError> {
    let raw = raw.trim();
    let found = match raw.parse::<Uuid>() {
        Ok(id) => collections::get(&state.db, caller.user_id, id)
            .await?
            .map(|c| c.id),
        Err(_) => collections::find_by_name(&state.db, caller.user_id, raw)
            .await?
            .map(|c| c.id),
    };
    found.ok_or_else(|| {
        ToolError::new(format!(
            "no collection {raw:?} in this library; call list_collections to see them"
        ))
    })
}

/// `resolve` for an optional argument.
pub async fn resolve_opt(
    state: &AppState,
    caller: Caller,
    raw: Option<&str>,
) -> Result<Option<Uuid>, ToolError> {
    match raw {
        Some(raw) => resolve(state, caller, raw).await.map(Some),
        None => Ok(None),
    }
}

pub async fn list(state: &AppState, caller: Caller) -> ToolResult {
    let rows = collections::list(&state.db, caller.user_id).await?;
    let items: Vec<_> = rows
        .into_iter()
        .map(|c| {
            json!({
                "collection_id": c.id,
                "name": c.name,
                "description": c.description.chars().take(DESCRIPTION_CHARS).collect::<String>(),
                "file_count": c.file_count,
            })
        })
        .collect();
    let mut out = json!({ "collections": items });
    if items_empty(&out) {
        out["note"] = json!("The user has no collections yet.");
    }
    Ok(out)
}

fn items_empty(out: &serde_json::Value) -> bool {
    out["collections"].as_array().is_some_and(Vec::is_empty)
}
