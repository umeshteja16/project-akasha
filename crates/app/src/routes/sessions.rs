//! `/api/v1/me/sessions`: where you are signed in, and signing other devices out.
//! Browser session only ([`SessionUser`]): API tokens get 403.

use axum::{
    extract::{Path, State},
    http::StatusCode,
};
use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::json;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    activity::{self, ActivityKind, Actor, ClientMeta},
    auth::SessionUser,
    error::{ApiError, ErrorBody},
    extract::Json,
    state::AppState,
};
use akasha_core::Error;
use akasha_db::sessions;

#[derive(Debug, Serialize, ToSchema)]
pub struct SessionResponse {
    pub id: Uuid,
    /// When this sign-in happened.
    pub created_at: DateTime<Utc>,
    pub last_seen_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    /// The browser's user agent at sign-in.
    pub user_agent: Option<String>,
    /// The address it signed in from.
    pub ip: Option<String>,
    /// This is the session making the request.
    pub current: bool,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct SessionList {
    /// Most recently used first.
    pub items: Vec<SessionResponse>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct SessionsRevoked {
    pub revoked: u64,
}

/// Where you are signed in.
#[utoipa::path(
    get, path = "/api/v1/me/sessions", tag = "me", operation_id = "list_sessions",
    responses(
        (status = 200, body = SessionList),
        (status = 401, body = ErrorBody), (status = 403, body = ErrorBody),
    ),
    security(("session_cookie" = []))
)]
pub async fn list(
    State(state): State<AppState>,
    auth: SessionUser,
) -> Result<Json<SessionList>, ApiError> {
    let rows = sessions::list(&state.db, auth.user_id).await?;
    Ok(Json(SessionList {
        items: rows
            .into_iter()
            .map(|s| SessionResponse {
                current: s.id == auth.session_id,
                id: s.id,
                created_at: s.created_at,
                last_seen_at: s.last_seen_at,
                expires_at: s.expires_at,
                user_agent: s.user_agent,
                ip: s.ip,
            })
            .collect(),
    }))
}

/// Sign out one of your other sessions (use `POST /auth/logout` for this one).
#[utoipa::path(
    delete, path = "/api/v1/me/sessions/{id}", tag = "me", operation_id = "revoke_session",
    params(("id" = Uuid, Path, description = "Session id")),
    responses(
        (status = 204, description = "Signed out"),
        (status = 400, body = ErrorBody), (status = 401, body = ErrorBody),
        (status = 403, body = ErrorBody), (status = 404, body = ErrorBody),
    ),
    security(("session_cookie" = []))
)]
pub async fn revoke(
    State(state): State<AppState>,
    auth: SessionUser,
    client: ClientMeta,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    if id == auth.session_id {
        return Err(Error::bad_request("this is your current session; sign out instead").into());
    }
    if !sessions::delete_owned(&state.db, auth.user_id, id).await? {
        return Err(Error::not_found("session not found").into());
    }
    let mut ev = activity::event(Actor::session(auth.user_id), ActivityKind::SessionRevoked);
    ev.details = json!({ "count": 1 });
    client.apply(&mut ev);
    activity::record_best_effort(&state.db, &ev).await;
    Ok(StatusCode::NO_CONTENT)
}

/// Sign out everywhere except here.
#[utoipa::path(
    post, path = "/api/v1/me/sessions/revoke-others", tag = "me",
    operation_id = "revoke_other_sessions",
    responses(
        (status = 200, body = SessionsRevoked),
        (status = 401, body = ErrorBody), (status = 403, body = ErrorBody),
    ),
    security(("session_cookie" = []))
)]
pub async fn revoke_others(
    State(state): State<AppState>,
    auth: SessionUser,
    client: ClientMeta,
) -> Result<Json<SessionsRevoked>, ApiError> {
    let revoked = sessions::delete_others(&state.db, auth.user_id, auth.session_id).await?;
    if revoked > 0 {
        let mut ev = activity::event(Actor::session(auth.user_id), ActivityKind::SessionRevoked);
        ev.details = json!({ "count": revoked, "others": true });
        client.apply(&mut ev);
        activity::record_best_effort(&state.db, &ev).await;
    }
    Ok(Json(SessionsRevoked { revoked }))
}
