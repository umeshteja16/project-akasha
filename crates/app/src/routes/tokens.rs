//! `/api/v1/me/tokens`: personal API tokens for MCP clients and scripts.
//!
//! Managed with the browser session only ([`SessionUser`]): a token can never
//! create, list or revoke tokens. The secret is returned once, at creation.

use axum::{
    extract::{Path, State},
    http::StatusCode,
};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    auth::{SessionUser, token},
    error::{ApiError, ErrorBody},
    extract::Json,
    state::AppState,
};
use akasha_core::Error;
use akasha_db::api_tokens::{self, ApiToken, NewToken};

/// Working (unrevoked, unexpired) tokens per user.
const MAX_ACTIVE: i64 = 50;
const NAME_MAX: usize = 100;
const MAX_EXPIRY_DAYS: u32 = 3650;

/// What a token may do.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum TokenScope {
    /// Search, list and read files, ask questions.
    Read,
    /// Also upload, change and delete files (requires `read`).
    Write,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct TokenResponse {
    pub id: Uuid,
    pub name: String,
    /// The first characters of the token, to recognise it.
    pub prefix: String,
    pub scopes: Vec<TokenScope>,
    pub created_at: DateTime<Utc>,
    /// Refreshed at most once a minute.
    pub last_used_at: Option<DateTime<Utc>>,
    /// `null`: never expires.
    pub expires_at: Option<DateTime<Utc>>,
    pub revoked_at: Option<DateTime<Utc>>,
}

impl From<ApiToken> for TokenResponse {
    fn from(t: ApiToken) -> Self {
        let scopes = token::Scopes::from_db(&t.scopes);
        let mut out = vec![TokenScope::Read];
        if scopes.write {
            out.push(TokenScope::Write);
        }
        Self {
            id: t.id,
            name: t.name,
            prefix: t.prefix,
            scopes: out,
            created_at: t.created_at,
            last_used_at: t.last_used_at,
            expires_at: t.expires_at,
            revoked_at: t.revoked_at,
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct TokenList {
    /// Newest first, revoked and expired ones included.
    pub items: Vec<TokenResponse>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateTokenRequest {
    /// What the token is for, e.g. "Claude Desktop on my laptop". 1-100 characters.
    pub name: String,
    /// `["read"]` (default) or `["read", "write"]`.
    pub scopes: Option<Vec<TokenScope>>,
    /// Days until the token stops working, 1-3650; `null`: never.
    pub expires_in_days: Option<u32>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct CreatedToken {
    pub token: TokenResponse,
    /// The token itself (`akasha_pat_…`). Shown only this once; store it safely.
    pub secret: String,
}

/// Your API tokens.
#[utoipa::path(
    get, path = "/api/v1/me/tokens", tag = "me", operation_id = "list_tokens",
    responses(
        (status = 200, body = TokenList),
        (status = 401, body = ErrorBody), (status = 403, body = ErrorBody),
    ),
    security(("session_cookie" = []))
)]
pub async fn list(
    State(state): State<AppState>,
    auth: SessionUser,
) -> Result<Json<TokenList>, ApiError> {
    let rows = api_tokens::list(&state.db, auth.user_id).await?;
    Ok(Json(TokenList {
        items: rows.into_iter().map(Into::into).collect(),
    }))
}

/// Create an API token for an MCP client or script. The secret is in the
/// response only; it cannot be shown again.
#[utoipa::path(
    post, path = "/api/v1/me/tokens", tag = "me", operation_id = "create_token",
    request_body = CreateTokenRequest,
    responses(
        (status = 201, body = CreatedToken),
        (status = 400, body = ErrorBody), (status = 401, body = ErrorBody),
        (status = 403, body = ErrorBody), (status = 409, body = ErrorBody),
    ),
    security(("session_cookie" = []))
)]
pub async fn create(
    State(state): State<AppState>,
    auth: SessionUser,
    Json(req): Json<CreateTokenRequest>,
) -> Result<(StatusCode, Json<CreatedToken>), ApiError> {
    let name = req.name.trim();
    if name.is_empty() || name.chars().count() > NAME_MAX {
        return Err(Error::bad_request(format!("name must be 1-{NAME_MAX} characters")).into());
    }
    let scopes = parse_scopes(req.scopes.as_deref())?;
    let expires_at = match req.expires_in_days {
        None => None,
        Some(days) if (1..=MAX_EXPIRY_DAYS).contains(&days) => {
            Some(Utc::now() + Duration::days(i64::from(days)))
        }
        Some(_) => {
            return Err(
                Error::bad_request(format!("expires_in_days must be 1-{MAX_EXPIRY_DAYS}")).into(),
            );
        }
    };
    if api_tokens::count_active(&state.db, auth.user_id).await? >= MAX_ACTIVE {
        return Err(Error::conflict(format!(
            "you already have {MAX_ACTIVE} active tokens; revoke one first"
        ))
        .into());
    }
    let secret = token::generate();
    let new = NewToken {
        owner_id: auth.user_id,
        name,
        token_hash: &secret.hash,
        prefix: &secret.prefix,
        scopes: &scopes.to_db(),
        expires_at,
    };
    let row = api_tokens::create(&state.db, &new).await?;
    tracing::info!(token_id = %row.id, write = scopes.write, "API token created");
    Ok((
        StatusCode::CREATED,
        Json(CreatedToken {
            token: row.into(),
            secret: secret.secret,
        }),
    ))
}

/// Revoke an API token. It stops working at once; revoking twice is harmless.
#[utoipa::path(
    delete, path = "/api/v1/me/tokens/{id}", tag = "me", operation_id = "revoke_token",
    params(("id" = Uuid, Path, description = "Token id")),
    responses(
        (status = 204, description = "Revoked"),
        (status = 401, body = ErrorBody), (status = 403, body = ErrorBody),
        (status = 404, body = ErrorBody),
    ),
    security(("session_cookie" = []))
)]
pub async fn revoke(
    State(state): State<AppState>,
    auth: SessionUser,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    if api_tokens::revoke(&state.db, auth.user_id, id).await? {
        tracing::info!(token_id = %id, "API token revoked");
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(Error::not_found("token not found").into())
    }
}

fn parse_scopes(raw: Option<&[TokenScope]>) -> Result<token::Scopes, Error> {
    let Some(raw) = raw else {
        return Ok(token::Scopes::READ);
    };
    if !raw.contains(&TokenScope::Read) {
        return Err(Error::bad_request("scopes must include \"read\""));
    }
    Ok(if raw.contains(&TokenScope::Write) {
        token::Scopes::READ_WRITE
    } else {
        token::Scopes::READ
    })
}
