//! Changing a file's name, pin or tags, and recording it in the activity
//! timeline in the same transaction (used by the REST route and MCP tools).

use akasha_db::files::{self, File, FileChanges};
use serde_json::json;
use uuid::Uuid;

use crate::{
    activity::{self, ActivityKind, Actor},
    error::ApiError,
    state::AppState,
};

/// Apply `changes` to one of the actor's files. `None`: no such file.
/// Renames and changes to the user's own tags become activity events; pins and
/// dropped suggestions do not (they are not worth a timeline entry).
pub async fn update(
    state: &AppState,
    actor: Actor,
    id: Uuid,
    changes: FileChanges<'_>,
) -> Result<Option<File>, ApiError> {
    let mut tx = state.db.begin().await?;
    let Some(before) = files::get(&mut *tx, actor.user_id, id).await? else {
        return Ok(None);
    };
    let Some(after) = files::update(&mut *tx, actor.user_id, id, changes).await? else {
        return Ok(None);
    };
    if after.original_name != before.original_name {
        let mut ev = activity::event(actor, ActivityKind::FileRenamed);
        ev.subject = Some(&after.original_name);
        ev.file_id = Some(after.id);
        ev.details = json!({ "from": before.original_name, "to": after.original_name });
        activity::record(&mut tx, &ev).await?;
    }
    let added: Vec<&String> = after
        .tags
        .iter()
        .filter(|t| !before.tags.contains(t))
        .collect();
    let removed: Vec<&String> = before
        .tags
        .iter()
        .filter(|t| !after.tags.contains(t))
        .collect();
    if !added.is_empty() || !removed.is_empty() {
        let mut ev = activity::event(actor, ActivityKind::FileTagged);
        ev.subject = Some(&after.original_name);
        ev.file_id = Some(after.id);
        ev.details = json!({ "added": added, "removed": removed });
        activity::record(&mut tx, &ev).await?;
    }
    tx.commit().await?;
    Ok(Some(after))
}

/// Mark a file opened (last_opened_at, open_count) and note it in the timeline,
/// at most once per file per 30 minutes. `None`: no such file.
pub async fn opened(state: &AppState, actor: Actor, id: Uuid) -> Result<Option<File>, ApiError> {
    const WINDOW_SECS: f64 = 30.0 * 60.0;
    let mut tx = state.db.begin().await?;
    let Some(file) = files::mark_opened(&mut tx, actor.user_id, id).await? else {
        return Ok(None);
    };
    let mut ev = activity::event(actor, ActivityKind::FileOpened);
    ev.subject = Some(&file.original_name);
    ev.file_id = Some(file.id);
    akasha_db::activity::record_unless_recent(&mut tx, &ev, WINDOW_SECS).await?;
    tx.commit().await?;
    Ok(Some(file))
}
