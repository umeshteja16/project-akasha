//! `/api/v1/auth`: register, log in, log out.

use axum::{extract::State, http::StatusCode};
use axum_extra::extract::CookieJar;
use chrono::{Duration, Utc};
use serde::Deserialize;
use utoipa::ToSchema;
use uuid::Uuid;

use super::me::UserResponse;
use crate::{
    activity::{self, ActivityKind, Actor, ClientMeta},
    auth::{SessionUser, password, session},
    error::{ApiError, ErrorBody},
    extract::Json,
    state::AppState,
};
use akasha_core::Error;
use akasha_db::users;

#[derive(Deserialize, ToSchema)]
pub struct RegisterRequest {
    pub email: String,
    pub password: String,
    pub display_name: Option<String>,
}

#[derive(Deserialize, ToSchema)]
pub struct LoginRequest {
    pub email: String,
    pub password: String,
}

/// Create an account and sign in.
#[utoipa::path(
    post, path = "/api/v1/auth/register", tag = "auth",
    request_body = RegisterRequest,
    responses(
        (status = 201, body = UserResponse, description = "Signed in; session cookie set"),
        (status = 400, body = ErrorBody), (status = 403, body = ErrorBody),
        (status = 409, body = ErrorBody), (status = 429, body = ErrorBody),
    )
)]
pub async fn register(
    State(state): State<AppState>,
    client: ClientMeta,
    jar: CookieJar,
    Json(req): Json<RegisterRequest>,
) -> Result<(StatusCode, CookieJar, Json<UserResponse>), ApiError> {
    if !state.config.allow_registration {
        return Err(Error::forbidden("registration is disabled on this server").into());
    }
    let email = normalize_email(&req.email)?;
    password::validate(&req.password)?;
    let display_name = super::me::normalize_display_name(req.display_name.as_deref())?;

    let hash = password::hash(req.password).await?;
    let user = match users::create(&state.db, &email, &hash, display_name.as_deref()).await? {
        users::Created::Ok(user) => user,
        users::Created::EmailTaken => {
            return Err(Error::conflict("an account with this email already exists").into());
        }
    };
    let jar = start_session(&state, &client, jar, user.id).await?;
    let mut ev = activity::event(Actor::session(user.id), ActivityKind::AccountCreated);
    client.apply(&mut ev);
    activity::record_best_effort(&state.db, &ev).await;
    Ok((StatusCode::CREATED, jar, Json(user.into())))
}

/// Sign in with email and password.
#[utoipa::path(
    post, path = "/api/v1/auth/login", tag = "auth",
    request_body = LoginRequest,
    responses(
        (status = 200, body = UserResponse, description = "Signed in; session cookie set"),
        (status = 400, body = ErrorBody), (status = 401, body = ErrorBody),
        (status = 429, body = ErrorBody),
    )
)]
pub async fn login(
    State(state): State<AppState>,
    client: ClientMeta,
    jar: CookieJar,
    Json(req): Json<LoginRequest>,
) -> Result<(CookieJar, Json<UserResponse>), ApiError> {
    let user = users::find_by_email(&state.db, req.email.trim()).await?;
    let valid =
        password::verify(req.password, user.as_ref().map(|u| u.password_hash.clone())).await;
    let user = match user {
        Some(user) if valid => user,
        Some(user) => {
            // The account's owner sees failed attempts in their security log.
            // Recorded in the background: the reply must take as long as for an
            // unknown email (no timing oracle for which accounts exist).
            let (db, meta, owner) = (state.db.clone(), client.clone(), user.id);
            tokio::spawn(async move {
                let mut ev = activity::event(Actor::session(owner), ActivityKind::SignInFailed);
                ev.via = None;
                meta.apply(&mut ev);
                activity::record_best_effort(&db, &ev).await;
            });
            tracing::info!(target: "audit", ip = ?client.ip, "sign-in failed");
            return Err(Error::unauthorized("incorrect email or password").into());
        }
        None => {
            tracing::info!(target: "audit", ip = ?client.ip, "sign-in failed (unknown email)");
            return Err(Error::unauthorized("incorrect email or password").into());
        }
    };
    let jar = start_session(&state, &client, jar, user.id).await?;
    let mut ev = activity::event(Actor::session(user.id), ActivityKind::SignedIn);
    client.apply(&mut ev);
    activity::record_best_effort(&state.db, &ev).await;
    Ok((jar, Json(user.into())))
}

/// End the current session.
#[utoipa::path(
    post, path = "/api/v1/auth/logout", tag = "auth",
    responses((status = 204, description = "Signed out"), (status = 401, body = ErrorBody)),
    security(("session_cookie" = []))
)]
pub async fn logout(
    State(state): State<AppState>,
    auth: SessionUser,
    client: ClientMeta,
    jar: CookieJar,
) -> Result<(StatusCode, CookieJar), ApiError> {
    akasha_db::sessions::delete(&state.db, auth.session_id).await?;
    let mut ev = activity::event(Actor::session(auth.user_id), ActivityKind::SignedOut);
    client.apply(&mut ev);
    activity::record_best_effort(&state.db, &ev).await;
    let jar = jar.add(session::removal(state.config.cookie_secure || client.https));
    Ok((StatusCode::NO_CONTENT, jar))
}

async fn start_session(
    state: &AppState,
    client: &ClientMeta,
    jar: CookieJar,
    user_id: Uuid,
) -> Result<CookieJar, ApiError> {
    let ttl = state.config.session_ttl_days;
    let (token, hash) = session::new_token();
    let expires_at = Utc::now() + Duration::days(i64::from(ttl));
    akasha_db::sessions::create(
        &state.db,
        user_id,
        &hash,
        expires_at,
        client.user_agent.as_deref(),
        client.ip.as_deref(),
    )
    .await?;
    Ok(jar.add(session::cookie(
        token,
        ttl,
        state.config.cookie_secure || client.https,
    )))
}

/// Basic sanity checks only; deliverability is not our concern.
fn normalize_email(raw: &str) -> Result<String, Error> {
    let email = raw.trim();
    let valid = email.len() <= 254
        && !email.chars().any(char::is_whitespace)
        && email
            .split_once('@')
            .is_some_and(|(local, domain)| !local.is_empty() && domain.contains('.'));
    if valid {
        Ok(email.to_owned())
    } else {
        Err(Error::bad_request("invalid email address"))
    }
}

#[cfg(test)]
mod tests {
    use super::normalize_email;

    #[test]
    fn email_validation() {
        assert_eq!(normalize_email("  a@b.co ").ok().as_deref(), Some("a@b.co"));
        for bad in ["", "a", "@b.co", "a@b", "a b@c.de"] {
            assert!(normalize_email(bad).is_err(), "{bad:?} should be rejected");
        }
    }
}
