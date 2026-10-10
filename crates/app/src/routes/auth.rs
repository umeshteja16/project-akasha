//! `/api/v1/auth`: register, log in, log out.

use axum::{
    extract::State,
    http::{HeaderMap, StatusCode, header::USER_AGENT},
};
use axum_extra::extract::CookieJar;
use chrono::{Duration, Utc};
use serde::Deserialize;
use utoipa::ToSchema;
use uuid::Uuid;

use super::me::UserResponse;
use crate::{
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
    headers: HeaderMap,
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
    let jar = start_session(&state, &headers, jar, user.id).await?;
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
    headers: HeaderMap,
    jar: CookieJar,
    Json(req): Json<LoginRequest>,
) -> Result<(CookieJar, Json<UserResponse>), ApiError> {
    let user = users::find_by_email(&state.db, req.email.trim()).await?;
    let valid =
        password::verify(req.password, user.as_ref().map(|u| u.password_hash.clone())).await;
    let user = match user {
        Some(user) if valid => user,
        _ => return Err(Error::unauthorized("incorrect email or password").into()),
    };
    let jar = start_session(&state, &headers, jar, user.id).await?;
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
    jar: CookieJar,
) -> Result<(StatusCode, CookieJar), ApiError> {
    akasha_db::sessions::delete(&state.db, auth.session_id).await?;
    let jar = jar.add(session::removal(state.config.cookie_secure));
    Ok((StatusCode::NO_CONTENT, jar))
}

async fn start_session(
    state: &AppState,
    headers: &HeaderMap,
    jar: CookieJar,
    user_id: Uuid,
) -> Result<CookieJar, ApiError> {
    let ttl = state.config.session_ttl_days;
    let (token, hash) = session::new_token();
    let user_agent = headers
        .get(USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .map(|ua| ua.chars().take(256).collect::<String>());
    let expires_at = Utc::now() + Duration::days(i64::from(ttl));
    akasha_db::sessions::create(&state.db, user_id, &hash, expires_at, user_agent.as_deref())
        .await?;
    Ok(jar.add(session::cookie(token, ttl, state.config.cookie_secure)))
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
