//! Tools that change the library (need the `write` scope): add_note, tag_file.

use akasha_db::files::{self, FileChanges};
use bytes::Bytes;
use serde_json::json;

use super::{
    Caller,
    output::{ToolError, ToolResult},
    tools::{NoteArgs, TagArgs},
};
use crate::{
    files::{
        name, sniff,
        store::{self, Saved},
    },
    routes::files::types::normalize_tags,
    state::AppState,
};
use akasha_core::Error;

const TITLE_MAX: usize = 200;
const CONTENT_MAX: usize = 200_000;

/// Save a Markdown note through the normal upload path (dedupe, quota,
/// extraction job), then tag it.
pub async fn add_note(state: &AppState, caller: Caller, a: NoteArgs) -> ToolResult {
    let title = a.title.trim();
    if title.is_empty() || title.chars().count() > TITLE_MAX {
        return Err(ToolError::new(format!(
            "title must be 1-{TITLE_MAX} characters"
        )));
    }
    let content = a.content.trim();
    if content.is_empty() || content.chars().count() > CONTENT_MAX {
        return Err(ToolError::new(format!(
            "content must be 1-{CONTENT_MAX} characters"
        )));
    }
    if content.contains('\0') {
        return Err(ToolError::new("content must not contain NUL characters"));
    }
    let tags = a.tags.as_deref().map(normalize_tags).transpose()?;
    // The title is part of the text, so it is searchable too.
    let body = if content.starts_with("# ") {
        format!("{content}\n")
    } else {
        format!("# {title}\n\n{content}\n")
    };
    let file_name = name::sanitize(&format!("{}.md", title.replace(['/', '\\'], "-")));
    let bytes = Bytes::from(body.into_bytes());
    if bytes.len() as u64 > state.config.max_upload_bytes() {
        return Err(Error::payload_too_large("note exceeds the upload limit").into());
    }
    let detected = sniff::detect(&bytes[..bytes.len().min(sniff::SNIFF_LEN)], &file_name)?;

    let mut staged = state.storage.stage().await.map_err(Error::from)?;
    if let Err(err) = staged.write(bytes).await {
        if let Err(abort) = staged.abort().await {
            tracing::warn!(err = %abort, "failed to abort a staged note");
        }
        return Err(Error::from(err).into());
    }
    let blob = staged.finish().await.map_err(Error::from)?;
    let (file, created) =
        match store::save(state, caller.user_id, blob, &file_name, detected.mime).await? {
            Saved::Created(f) => (f, true),
            Saved::Existing(f) => (f, false),
        };
    let file = match tags.filter(|t| !t.is_empty()) {
        Some(new_tags) => {
            let mut merged = file.tags.clone();
            merged.extend(new_tags.into_iter().filter(|t| !file.tags.contains(t)));
            let merged = normalize_tags(&merged)?;
            let changes = FileChanges {
                original_name: None,
                is_pinned: None,
                tags: Some(&merged),
                auto_tags: None,
            };
            files::update(&state.db, caller.user_id, file.id, changes)
                .await?
                .unwrap_or(file)
        }
        None => file,
    };
    tracing::info!(file_id = %file.id, created, "MCP note added");
    Ok(json!({
        "file_id": file.id,
        "name": file.original_name,
        "status": file.status,
        "tags": file.tags,
        "created": created,
        "note": if created {
            "Saved. It is being indexed and will be searchable shortly."
        } else {
            "The library already has a file with exactly this content; nothing was added."
        },
    }))
}

pub async fn tag_file(state: &AppState, caller: Caller, a: TagArgs) -> ToolResult {
    let add = normalize_tags(a.add.as_deref().unwrap_or_default())?;
    let remove = normalize_tags(a.remove.as_deref().unwrap_or_default())?;
    if add.is_empty() && remove.is_empty() {
        return Err(ToolError::new("give tags to `add` and/or `remove`"));
    }
    let not_found = || ToolError::new(format!("no file with id {} in this library", a.file_id));
    let file = files::get(&state.db, caller.user_id, a.file_id)
        .await?
        .ok_or_else(not_found)?;
    let mut tags: Vec<String> = file
        .tags
        .into_iter()
        .filter(|t| !remove.contains(t))
        .collect();
    tags.extend(
        add.into_iter()
            .filter(|t| !tags.contains(t))
            .collect::<Vec<_>>(),
    );
    let tags = normalize_tags(&tags)?;
    let changes = FileChanges {
        original_name: None,
        is_pinned: None,
        tags: Some(&tags),
        auto_tags: None,
    };
    let file = files::update(&state.db, caller.user_id, a.file_id, changes)
        .await?
        .ok_or_else(not_found)?;
    Ok(json!({ "file_id": file.id, "name": file.original_name, "tags": file.tags }))
}
