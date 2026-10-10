//! `/api/v1/collections`: named groups of your files. A file can be in any
//! number of collections; deleting a collection keeps its files. Every query is
//! filtered by the signed-in owner; other users' collections are 404.
//!
//! List a collection's files with `GET /api/v1/files?collection_id=`, search in
//! it with `GET /api/v1/search?collection_id=`.

pub mod types;

use axum::{
    extract::{Path, State},
    http::StatusCode,
};
use serde_json::json;
use uuid::Uuid;

use self::types::{
    CollectionColor, CollectionFilesChanged, CollectionFilesRequest, CollectionIcon,
    CollectionList, CollectionResponse, CreateCollection, MAX_COLLECTIONS, UpdateCollection,
    check_ids, clean_description, clean_name,
};
use crate::{
    activity::{self, ActivityKind, Actor},
    auth::AuthUser,
    error::{ApiError, ErrorBody},
    extract::Json,
    state::AppState,
};
use akasha_core::Error;
use akasha_db::collections::{self, Collection, CollectionChanges, FileName, NewCollection, Saved};

pub(crate) fn not_found() -> ApiError {
    Error::not_found("collection not found").into()
}

fn name_taken() -> ApiError {
    Error::conflict("you already have a collection with this name").into()
}

/// 404 unless the signed-in user has this collection.
pub(crate) async fn ensure_owned(state: &AppState, owner: Uuid, id: Uuid) -> Result<(), ApiError> {
    if collections::exists(&state.db, owner, id).await? {
        Ok(())
    } else {
        Err(not_found())
    }
}

/// Your collections, by name.
#[utoipa::path(
    get, path = "/api/v1/collections", tag = "collections", operation_id = "list_collections",
    responses((status = 200, body = CollectionList), (status = 401, body = ErrorBody)),
    security(("session_cookie" = []), ("api_token" = []))
)]
pub async fn list(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<CollectionList>, ApiError> {
    let rows = collections::list(&state.db, auth.user_id).await?;
    Ok(Json(CollectionList {
        items: rows.into_iter().map(Into::into).collect(),
    }))
}

/// Create a collection, optionally with files in it.
#[utoipa::path(
    post, path = "/api/v1/collections", tag = "collections", operation_id = "create_collection",
    request_body = CreateCollection,
    responses(
        (status = 201, body = CollectionResponse),
        (status = 400, body = ErrorBody), (status = 401, body = ErrorBody),
        (status = 403, body = ErrorBody), (status = 409, body = ErrorBody),
    ),
    security(("session_cookie" = []), ("api_token" = []))
)]
pub async fn create(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<CreateCollection>,
) -> Result<(StatusCode, Json<CollectionResponse>), ApiError> {
    let name = clean_name(&body.name)?;
    let description = clean_description(body.description.as_deref().unwrap_or(""))?;
    let file_ids = body.file_ids.unwrap_or_default();
    check_ids(&file_ids)?;
    if collections::count(&state.db, auth.user_id).await? >= MAX_COLLECTIONS {
        return Err(Error::conflict(format!(
            "you already have {MAX_COLLECTIONS} collections; delete one first"
        ))
        .into());
    }
    let actor = Actor::from(&auth);
    let mut tx = state.db.begin().await?;
    let new = NewCollection {
        owner_id: auth.user_id,
        name: &name,
        description: &description,
        color: body.color.unwrap_or(CollectionColor::Sage).as_str(),
        icon: body.icon.unwrap_or(CollectionIcon::Folder).as_str(),
    };
    let created = match collections::create(&mut tx, &new).await? {
        Saved::Ok(c) => c,
        Saved::NameTaken => return Err(name_taken()),
        Saved::NotFound => return Err(not_found()),
    };
    let mut ev = activity::event(actor, ActivityKind::CollectionCreated);
    ev.subject = Some(&created.name);
    ev.collection_id = Some(created.id);
    activity::record(&mut tx, &ev).await?;
    let mut created = created;
    if !file_ids.is_empty() {
        let added = collections::add_files(&mut tx, auth.user_id, created.id, &file_ids).await?;
        created.file_count = i64::try_from(added.len()).unwrap_or(i64::MAX);
        record_files(&mut tx, actor, &created, &added, true).await?;
    }
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(created.into())))
}

/// One of your collections.
#[utoipa::path(
    get, path = "/api/v1/collections/{id}", tag = "collections", operation_id = "get_collection",
    params(("id" = Uuid, Path, description = "Collection id")),
    responses(
        (status = 200, body = CollectionResponse),
        (status = 401, body = ErrorBody), (status = 404, body = ErrorBody),
    ),
    security(("session_cookie" = []), ("api_token" = []))
)]
pub async fn get(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<CollectionResponse>, ApiError> {
    let c = collections::get(&state.db, auth.user_id, id)
        .await?
        .ok_or_else(not_found)?;
    Ok(Json(c.into()))
}

/// Rename, describe, recolour or change the icon of a collection.
#[utoipa::path(
    patch, path = "/api/v1/collections/{id}", tag = "collections", operation_id = "update_collection",
    params(("id" = Uuid, Path, description = "Collection id")),
    request_body = UpdateCollection,
    responses(
        (status = 200, body = CollectionResponse),
        (status = 400, body = ErrorBody), (status = 401, body = ErrorBody),
        (status = 403, body = ErrorBody), (status = 404, body = ErrorBody),
        (status = 409, body = ErrorBody),
    ),
    security(("session_cookie" = []), ("api_token" = []))
)]
pub async fn update(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateCollection>,
) -> Result<Json<CollectionResponse>, ApiError> {
    let name = body.name.as_deref().map(clean_name).transpose()?;
    let description = body
        .description
        .as_deref()
        .map(clean_description)
        .transpose()?;
    if name.is_none() && description.is_none() && body.color.is_none() && body.icon.is_none() {
        return Err(Error::bad_request("nothing to update").into());
    }
    let before = collections::get(&state.db, auth.user_id, id)
        .await?
        .ok_or_else(not_found)?;
    let changes = CollectionChanges {
        name: name.as_deref(),
        description: description.as_deref(),
        color: body.color.map(CollectionColor::as_str),
        icon: body.icon.map(CollectionIcon::as_str),
    };
    let mut tx = state.db.begin().await?;
    let updated = match collections::update(&mut tx, auth.user_id, id, changes).await? {
        Saved::Ok(c) => c,
        Saved::NameTaken => return Err(name_taken()),
        Saved::NotFound => return Err(not_found()),
    };
    let mut changed = Vec::new();
    if updated.name != before.name {
        changed.push("name");
    }
    if updated.description != before.description {
        changed.push("description");
    }
    if updated.color != before.color || updated.icon != before.icon {
        changed.push("look");
    }
    if !changed.is_empty() {
        let mut ev = activity::event(Actor::from(&auth), ActivityKind::CollectionUpdated);
        ev.subject = Some(&updated.name);
        ev.collection_id = Some(updated.id);
        ev.details = if updated.name == before.name {
            json!({ "changed": changed })
        } else {
            json!({ "changed": changed, "from": before.name, "to": updated.name })
        };
        activity::record(&mut tx, &ev).await?;
    }
    tx.commit().await?;
    Ok(Json(updated.into()))
}

/// Delete a collection. Its files are kept.
#[utoipa::path(
    delete, path = "/api/v1/collections/{id}", tag = "collections", operation_id = "delete_collection",
    params(("id" = Uuid, Path, description = "Collection id")),
    responses(
        (status = 204, description = "Deleted"),
        (status = 401, body = ErrorBody), (status = 403, body = ErrorBody),
        (status = 404, body = ErrorBody),
    ),
    security(("session_cookie" = []), ("api_token" = []))
)]
pub async fn delete(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    let mut tx = state.db.begin().await?;
    let name = collections::delete(&mut tx, auth.user_id, id)
        .await?
        .ok_or_else(not_found)?;
    let mut ev = activity::event(Actor::from(&auth), ActivityKind::CollectionDeleted);
    ev.subject = Some(&name);
    activity::record(&mut tx, &ev).await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Add files to a collection (up to 100 per request). Files already in it, and
/// ids that are not your files, are skipped.
#[utoipa::path(
    post, path = "/api/v1/collections/{id}/files", tag = "collections",
    operation_id = "add_collection_files",
    params(("id" = Uuid, Path, description = "Collection id")),
    request_body = CollectionFilesRequest,
    responses(
        (status = 200, body = CollectionFilesChanged),
        (status = 400, body = ErrorBody), (status = 401, body = ErrorBody),
        (status = 403, body = ErrorBody), (status = 404, body = ErrorBody),
    ),
    security(("session_cookie" = []), ("api_token" = []))
)]
pub async fn add_files(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<CollectionFilesRequest>,
) -> Result<Json<CollectionFilesChanged>, ApiError> {
    change_files(&state, &auth, id, &body.file_ids, true).await
}

/// Take files out of a collection (the files themselves are kept).
#[utoipa::path(
    post, path = "/api/v1/collections/{id}/files/remove", tag = "collections",
    operation_id = "remove_collection_files",
    params(("id" = Uuid, Path, description = "Collection id")),
    request_body = CollectionFilesRequest,
    responses(
        (status = 200, body = CollectionFilesChanged),
        (status = 400, body = ErrorBody), (status = 401, body = ErrorBody),
        (status = 403, body = ErrorBody), (status = 404, body = ErrorBody),
    ),
    security(("session_cookie" = []), ("api_token" = []))
)]
pub async fn remove_files(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<CollectionFilesRequest>,
) -> Result<Json<CollectionFilesChanged>, ApiError> {
    change_files(&state, &auth, id, &body.file_ids, false).await
}

async fn change_files(
    state: &AppState,
    auth: &AuthUser,
    id: Uuid,
    file_ids: &[Uuid],
    add: bool,
) -> Result<Json<CollectionFilesChanged>, ApiError> {
    check_ids(file_ids)?;
    let mut tx = state.db.begin().await?;
    let changed = if add {
        collections::add_files(&mut tx, auth.user_id, id, file_ids).await?
    } else {
        collections::remove_files(&mut tx, auth.user_id, id, file_ids).await?
    };
    let collection = collections::get(&mut *tx, auth.user_id, id)
        .await?
        .ok_or_else(not_found)?;
    if !changed.is_empty() {
        record_files(&mut tx, Actor::from(auth), &collection, &changed, add).await?;
    }
    tx.commit().await?;
    Ok(Json(CollectionFilesChanged {
        file_ids: changed.into_iter().map(|f| f.id).collect(),
        collection: collection.into(),
    }))
}

/// One event per change: "Added report.pdf to Trips" or "Added 3 files to Trips".
async fn record_files(
    conn: &mut sqlx::PgConnection,
    actor: Actor,
    collection: &Collection,
    files: &[FileName],
    added: bool,
) -> Result<(), sqlx::Error> {
    let kind = if added {
        ActivityKind::CollectionFilesAdded
    } else {
        ActivityKind::CollectionFilesRemoved
    };
    let mut ev = activity::event(actor, kind);
    ev.subject = Some(&collection.name);
    ev.collection_id = Some(collection.id);
    let names: Vec<&str> = files.iter().take(5).map(|f| f.name.as_str()).collect();
    ev.details = json!({ "count": files.len(), "file_names": names });
    if let [one] = files {
        ev.file_id = Some(one.id);
    }
    activity::record(conn, &ev).await
}
