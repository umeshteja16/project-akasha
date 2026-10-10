//! `/api/v1/sources`: watched folders. A source is a folder on the server, inside one
//! of the admin's `AKASHA_WATCH_ROOTS`, whose files are imported into your library and
//! kept in sync (new, changed, renamed and deleted files). Every query is filtered by
//! the signed-in owner; other users' sources are 404.

pub mod types;

use std::path::PathBuf;

use axum::{
    extract::{Path, State},
    http::StatusCode,
};
use serde_json::json;
use uuid::Uuid;

use self::types::{
    CreateSource, DeleteSourceQuery, MAX_SOURCES, SourceList, SourceOnDelete, SourceRemoved,
    SourceResponse, UpdateSource, clean_globs, clean_name,
};
use crate::{
    activity::{self, ActivityKind, Actor},
    auth::AuthUser,
    error::{ApiError, ErrorBody},
    extract::{Json, Query},
    jobs::{blobs, kinds::ScanSource},
    sources::paths,
    state::AppState,
};
use akasha_core::Error;
use akasha_db::{
    files,
    sources::{self, NewSource, SourceChanges},
};

fn not_found() -> ApiError {
    Error::not_found("source not found").into()
}

/// The user's canonical watch roots (blocking filesystem calls off the runtime).
async fn roots(state: &AppState, user_id: Uuid) -> Result<Vec<PathBuf>, ApiError> {
    let user = akasha_db::users::find_by_id(&state.db, user_id)
        .await?
        .ok_or_else(|| Error::unauthorized("account no longer exists"))?;
    let configured = state.config.watch_roots.clone();
    tokio::task::spawn_blocking(move || paths::roots_for(&configured, user.id, &user.email))
        .await
        .map_err(|_| Error::internal("internal error").into())
}

async fn respond(state: &AppState, owner: Uuid, id: Uuid) -> Result<SourceResponse, ApiError> {
    Ok(sources::summary(&state.db, owner, id)
        .await?
        .ok_or_else(not_found)?
        .into())
}

/// Your watched folders, and the folders you may add.
#[utoipa::path(
    get, path = "/api/v1/sources", tag = "sources", operation_id = "list_sources",
    responses((status = 200, body = SourceList), (status = 401, body = ErrorBody)),
    security(("session_cookie" = []), ("api_token" = []))
)]
pub async fn list(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<SourceList>, ApiError> {
    let rows = sources::list(&state.db, auth.user_id).await?;
    let roots = roots(&state, auth.user_id).await?;
    Ok(Json(SourceList {
        items: rows.into_iter().map(Into::into).collect(),
        roots: roots.iter().map(|r| r.display().to_string()).collect(),
    }))
}

/// Watch a folder: its files are imported now and kept in sync.
///
/// The path must be inside one of `roots` after resolving `..` and symlinks (`403`
/// otherwise). Folders inside or around one you already watch are refused (`409`).
#[utoipa::path(
    post, path = "/api/v1/sources", tag = "sources", operation_id = "create_source",
    request_body = CreateSource,
    responses(
        (status = 201, body = SourceResponse),
        (status = 400, body = ErrorBody), (status = 401, body = ErrorBody),
        (status = 403, body = ErrorBody), (status = 409, body = ErrorBody),
    ),
    security(("session_cookie" = []), ("api_token" = []))
)]
pub async fn create(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<CreateSource>,
) -> Result<(StatusCode, Json<SourceResponse>), ApiError> {
    let include = clean_globs(&body.include_globs.unwrap_or_default())?;
    let exclude = clean_globs(&body.exclude_globs.unwrap_or_default())?;
    let roots = roots(&state, auth.user_id).await?;
    let requested = body.path.clone();
    let canonical = tokio::task::spawn_blocking(move || paths::resolve(&requested, &roots))
        .await
        .map_err(|_| Error::internal("internal error"))??;
    let path = canonical
        .to_str()
        .ok_or_else(|| Error::bad_request("the folder path must be UTF-8"))?
        .to_owned();
    let name = match body.name.as_deref() {
        Some(name) => clean_name(name)?,
        None => clean_name(
            canonical
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("Folder"),
        )?,
    };
    let existing = sources::list(&state.db, auth.user_id).await?;
    if existing.len() >= MAX_SOURCES {
        return Err(Error::conflict(format!(
            "you already watch {MAX_SOURCES} folders; remove one first"
        ))
        .into());
    }
    if let Some(other) = existing.iter().find(|s| {
        let other = std::path::Path::new(&s.source.path);
        canonical.starts_with(other) || other.starts_with(&canonical)
    }) {
        return Err(Error::conflict(format!(
            "this folder overlaps with \"{}\", which you already watch",
            other.source.name
        ))
        .into());
    }
    let mut tx = state.db.begin().await?;
    let new = NewSource {
        owner_id: auth.user_id,
        name: &name,
        path: &path,
        include_globs: &include,
        exclude_globs: &exclude,
        on_delete: body.on_delete.unwrap_or(SourceOnDelete::Delete).as_str(),
        import_tags: body.import_tags.unwrap_or(true),
    };
    let source = sources::create(&mut tx, &new)
        .await?
        .ok_or_else(|| Error::conflict("you already watch this folder"))?;
    akasha_jobs::enqueue(&mut tx, &ScanSource::new(source.id)).await?;
    let mut ev = activity::event(Actor::from(&auth), ActivityKind::SourceAdded);
    ev.subject = Some(&source.name);
    ev.details = json!({ "source_id": source.id, "path": source.path });
    activity::record(&mut tx, &ev).await?;
    tx.commit().await?;
    Ok((
        StatusCode::CREATED,
        Json(respond(&state, auth.user_id, source.id).await?),
    ))
}

/// One of your watched folders.
#[utoipa::path(
    get, path = "/api/v1/sources/{id}", tag = "sources", operation_id = "get_source",
    params(("id" = Uuid, Path, description = "Source id")),
    responses(
        (status = 200, body = SourceResponse),
        (status = 401, body = ErrorBody), (status = 404, body = ErrorBody),
    ),
    security(("session_cookie" = []), ("api_token" = []))
)]
pub async fn get(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<SourceResponse>, ApiError> {
    Ok(Json(respond(&state, auth.user_id, id).await?))
}

/// Rename, change filters or deletion handling, pause or resume a watched folder.
/// Resuming or changing the globs scans it again.
#[utoipa::path(
    patch, path = "/api/v1/sources/{id}", tag = "sources", operation_id = "update_source",
    params(("id" = Uuid, Path, description = "Source id")),
    request_body = UpdateSource,
    responses(
        (status = 200, body = SourceResponse),
        (status = 400, body = ErrorBody), (status = 401, body = ErrorBody),
        (status = 404, body = ErrorBody),
    ),
    security(("session_cookie" = []), ("api_token" = []))
)]
pub async fn update(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateSource>,
) -> Result<Json<SourceResponse>, ApiError> {
    let name = body.name.as_deref().map(clean_name).transpose()?;
    let include = body.include_globs.as_deref().map(clean_globs).transpose()?;
    let exclude = body.exclude_globs.as_deref().map(clean_globs).transpose()?;
    let mut tx = state.db.begin().await?;
    let before = sources::get(&mut *tx, auth.user_id, id)
        .await?
        .ok_or_else(not_found)?;
    let changes = SourceChanges {
        name: name.as_deref(),
        include_globs: include.as_deref(),
        exclude_globs: exclude.as_deref(),
        on_delete: body.on_delete.map(SourceOnDelete::as_str),
        import_tags: body.import_tags,
        enabled: body.enabled,
    };
    let after = sources::update(&mut *tx, auth.user_id, id, changes)
        .await?
        .ok_or_else(not_found)?;
    let rescan = after.enabled
        && (!before.enabled
            || before.include_globs != after.include_globs
            || before.exclude_globs != after.exclude_globs);
    if rescan {
        akasha_jobs::enqueue(&mut tx, &ScanSource::new(id)).await?;
    }
    tx.commit().await?;
    Ok(Json(respond(&state, auth.user_id, id).await?))
}

/// Scan a watched folder now (`409` while it is paused).
#[utoipa::path(
    post, path = "/api/v1/sources/{id}/scan", tag = "sources", operation_id = "scan_source",
    params(("id" = Uuid, Path, description = "Source id")),
    responses(
        (status = 202, description = "Scan queued", body = SourceResponse),
        (status = 401, body = ErrorBody), (status = 404, body = ErrorBody),
        (status = 409, body = ErrorBody),
    ),
    security(("session_cookie" = []), ("api_token" = []))
)]
pub async fn scan(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<(StatusCode, Json<SourceResponse>), ApiError> {
    let source = sources::get(&state.db, auth.user_id, id)
        .await?
        .ok_or_else(not_found)?;
    if !source.enabled {
        return Err(Error::conflict("this folder is paused; resume it first").into());
    }
    let mut conn = state.db.acquire().await?;
    akasha_jobs::enqueue(&mut conn, &ScanSource::new(id)).await?;
    Ok((
        StatusCode::ACCEPTED,
        Json(respond(&state, auth.user_id, id).await?),
    ))
}

/// Stop watching a folder. Its files stay in your library unless `delete_files=true`
/// (files you also had before, or that another folder maps, always stay).
#[utoipa::path(
    delete, path = "/api/v1/sources/{id}", tag = "sources", operation_id = "delete_source",
    params(("id" = Uuid, Path, description = "Source id"), DeleteSourceQuery),
    responses(
        (status = 200, body = SourceRemoved),
        (status = 401, body = ErrorBody), (status = 404, body = ErrorBody),
    ),
    security(("session_cookie" = []), ("api_token" = []))
)]
pub async fn delete(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Query(query): Query<DeleteSourceQuery>,
) -> Result<Json<SourceRemoved>, ApiError> {
    let mut tx = state.db.begin().await?;
    let source = sources::get(&mut *tx, auth.user_id, id)
        .await?
        .ok_or_else(not_found)?;
    let ids = if query.delete_files {
        sources::created_files(&mut tx, auth.user_id, id).await?
    } else {
        Vec::new()
    };
    sources::delete(&mut tx, auth.user_id, id).await?;
    let rows = files::delete_many(&mut tx, auth.user_id, &ids).await?;
    let hashes: Vec<String> = rows.iter().map(|r| r.content_hash.clone()).collect();
    blobs::release(&mut tx, &hashes).await?;
    let mut ev = activity::event(Actor::from(&auth), ActivityKind::SourceRemoved);
    ev.subject = Some(&source.name);
    ev.details = json!({ "source_id": id, "deleted_files": rows.len() });
    activity::record(&mut tx, &ev).await?;
    tx.commit().await?;
    Ok(Json(SourceRemoved {
        deleted_files: rows.len(),
    }))
}
