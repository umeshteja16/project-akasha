//! `/api/v1/me`: the signed-in user's own account.

use axum::{extract::State, http::StatusCode};
use axum_extra::extract::CookieJar;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    activity::{self, ActivityKind, Actor, ClientMeta},
    auth::{AuthUser, SessionUser, password, session},
    error::{ApiError, ErrorBody},
    extract::Json,
    state::AppState,
};
use akasha_core::Error;
use akasha_db::{sessions, users};

const DISPLAY_NAME_MAX: usize = 100;

#[derive(Serialize, ToSchema)]
pub struct UserResponse {
    pub id: Uuid,
    pub email: String,
    pub display_name: Option<String>,
    pub created_at: DateTime<Utc>,
    /// Searches (with their query) appear in the activity timeline.
    pub record_search_history: bool,
}

impl From<users::User> for UserResponse {
    fn from(user: users::User) -> Self {
        Self {
            id: user.id,
            email: user.email,
            display_name: user.display_name,
            created_at: user.created_at,
            record_search_history: user.record_search_history,
        }
    }
}

#[derive(Deserialize, ToSchema)]
pub struct UpdateMeRequest {
    /// New display name; `null` or blank clears it, absent keeps it.
    #[serde(default, deserialize_with = "crate::extract::double_option")]
    #[schema(value_type = Option<String>)]
    pub display_name: Option<Option<String>>,
    /// Keep a history of your searches (with the query text) in the activity
    /// timeline. Turning it off stops recording; existing entries stay until
    /// you clear them.
    pub record_search_history: Option<bool>,
}

#[derive(Deserialize, ToSchema)]
pub struct ChangePasswordRequest {
    pub current_password: String,
    pub new_password: String,
}

#[derive(Deserialize, ToSchema)]
pub struct DeleteAccountRequest {
    /// Current password, to confirm.
    pub password: String,
}

/// The signed-in user.
#[utoipa::path(
    get, path = "/api/v1/me", tag = "me",
    responses((status = 200, body = UserResponse), (status = 401, body = ErrorBody)),
    security(("session_cookie" = []))
)]
pub async fn get_me(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<UserResponse>, ApiError> {
    Ok(Json(load(&state, auth.user_id).await?.into()))
}

/// Update profile fields.
#[utoipa::path(
    patch, path = "/api/v1/me", tag = "me",
    request_body = UpdateMeRequest,
    responses(
        (status = 200, body = UserResponse),
        (status = 400, body = ErrorBody), (status = 401, body = ErrorBody),
    ),
    security(("session_cookie" = []))
)]
pub async fn update_me(
    State(state): State<AppState>,
    auth: SessionUser,
    Json(req): Json<UpdateMeRequest>,
) -> Result<Json<UserResponse>, ApiError> {
    if req.display_name.is_none() && req.record_search_history.is_none() {
        return Err(Error::bad_request("nothing to update").into());
    }
    if let Some(on) = req.record_search_history {
        akasha_db::activity::set_search_history(&state.db, auth.user_id, on).await?;
    }
    if let Some(raw) = &req.display_name {
        let display_name = normalize_display_name(raw.as_deref())?;
        users::update_display_name(&state.db, auth.user_id, display_name.as_deref()).await?;
    }
    Ok(Json(load(&state, auth.user_id).await?.into()))
}

/// Change password. Signs out every other session.
#[utoipa::path(
    post, path = "/api/v1/me/password", tag = "me",
    request_body = ChangePasswordRequest,
    responses(
        (status = 204, description = "Password changed"),
        (status = 400, body = ErrorBody), (status = 401, body = ErrorBody),
        (status = 429, body = ErrorBody),
    ),
    security(("session_cookie" = []))
)]
pub async fn change_password(
    State(state): State<AppState>,
    auth: SessionUser,
    client: ClientMeta,
    Json(req): Json<ChangePasswordRequest>,
) -> Result<StatusCode, ApiError> {
    let user = load(&state, auth.user_id).await?;
    let actor = Actor::session(user.id);
    if !password::verify(req.current_password, Some(user.password_hash)).await {
        let mut ev = activity::event(actor, ActivityKind::PasswordChangeFailed);
        client.apply(&mut ev);
        activity::record_best_effort(&state.db, &ev).await;
        return Err(Error::unauthorized("current password is incorrect").into());
    }
    password::validate(&req.new_password)?;
    let hash = password::hash(req.new_password).await?;
    users::update_password_hash(&state.db, user.id, &hash).await?;
    let revoked = sessions::delete_others(&state.db, user.id, auth.session_id).await?;
    let mut ev = activity::event(actor, ActivityKind::PasswordChanged);
    ev.details = serde_json::json!({ "sessions_revoked": revoked });
    client.apply(&mut ev);
    activity::record_best_effort(&state.db, &ev).await;
    Ok(StatusCode::NO_CONTENT)
}

/// Permanently delete the account and everything it owns.
#[utoipa::path(
    delete, path = "/api/v1/me", tag = "me",
    request_body = DeleteAccountRequest,
    responses(
        (status = 204, description = "Account deleted; session cookie cleared"),
        (status = 401, body = ErrorBody), (status = 429, body = ErrorBody),
    ),
    security(("session_cookie" = []))
)]
pub async fn delete_me(
    State(state): State<AppState>,
    auth: SessionUser,
    client: ClientMeta,
    jar: CookieJar,
    Json(req): Json<DeleteAccountRequest>,
) -> Result<(StatusCode, CookieJar), ApiError> {
    let user = load(&state, auth.user_id).await?;
    if !password::verify(req.password, Some(user.password_hash)).await {
        let mut ev = activity::event(Actor::session(user.id), ActivityKind::AccountDeleteFailed);
        client.apply(&mut ev);
        activity::record_best_effort(&state.db, &ev).await;
        return Err(Error::unauthorized("password is incorrect").into());
    }
    // The account's activity goes with it, so the deletion is audited in the
    // server log only.
    tracing::info!(target: "audit", user_id = %user.id, ip = ?client.ip, "account deleted");
    // Rows go with the user (ON DELETE CASCADE); blobs are released by jobs
    // enqueued in the same transaction.
    let mut tx = state.db.begin().await?;
    let hashes = akasha_db::files::hashes_owned_by(&mut tx, user.id).await?;
    users::delete(&mut *tx, user.id).await?;
    crate::jobs::blobs::release(&mut tx, &hashes).await?;
    tx.commit().await?;
    let jar = jar.add(session::removal(state.config.cookie_secure));
    Ok((StatusCode::NO_CONTENT, jar))
}

async fn load(state: &AppState, user_id: Uuid) -> Result<users::User, ApiError> {
    users::find_by_id(&state.db, user_id)
        .await?
        .ok_or_else(|| Error::unauthorized("account no longer exists").into())
}

/// Trim; blank means "no display name".
pub fn normalize_display_name(raw: Option<&str>) -> Result<Option<String>, Error> {
    let Some(name) = raw.map(str::trim).filter(|n| !n.is_empty()) else {
        return Ok(None);
    };
    if name.chars().count() > DISPLAY_NAME_MAX {
        return Err(Error::bad_request(format!(
            "display name must be at most {DISPLAY_NAME_MAX} characters"
        )));
    }
    Ok(Some(name.to_owned()))
}
