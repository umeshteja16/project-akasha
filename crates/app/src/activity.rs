//! The activity timeline and security audit log: what each kind of event is
//! called, which category it belongs to, and helpers to record one.
//!
//! Record inside the transaction of the action whenever there is one
//! ([`record`]), so an event exists exactly when the change committed.
//! Otherwise (sign-ins, opens, searches) record best-effort ([`record_best_effort`]):
//! a failure is logged and never fails the request.
//!
//! Privacy: events are only ever listed to their owner (`GET /api/v1/activity`).
//! Searches keep the query text unless the owner turns search history off.

use axum::{
    extract::FromRequestParts,
    http::{header::USER_AGENT, request::Parts},
};
use serde::{Deserialize, Serialize};
use sqlx::PgConnection;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    auth::{AuthUser, Credential},
    client_ip::ClientAddr,
};
use akasha_db::{PgPool, activity::NewEvent};

/// Broad groups of events (the timeline's filters).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum ActivityCategory {
    Files,
    Search,
    Chat,
    Collections,
    /// The security audit log: sign-ins, passwords, sessions, tokens, rate limits.
    Security,
}

impl ActivityCategory {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Files => "files",
            Self::Search => "search",
            Self::Chat => "chat",
            Self::Collections => "collections",
            Self::Security => "security",
        }
    }
}

/// What happened.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub enum ActivityKind {
    #[serde(rename = "file.uploaded")]
    FileUploaded,
    #[serde(rename = "file.renamed")]
    FileRenamed,
    #[serde(rename = "file.tagged")]
    FileTagged,
    #[serde(rename = "file.deleted")]
    FileDeleted,
    #[serde(rename = "file.opened")]
    FileOpened,
    #[serde(rename = "source.added")]
    SourceAdded,
    #[serde(rename = "source.removed")]
    SourceRemoved,
    #[serde(rename = "source.synced")]
    SourceSynced,
    #[serde(rename = "search.performed")]
    SearchPerformed,
    #[serde(rename = "chat.asked")]
    ChatAsked,
    #[serde(rename = "collection.created")]
    CollectionCreated,
    #[serde(rename = "collection.updated")]
    CollectionUpdated,
    #[serde(rename = "collection.deleted")]
    CollectionDeleted,
    #[serde(rename = "collection.files_added")]
    CollectionFilesAdded,
    #[serde(rename = "collection.files_removed")]
    CollectionFilesRemoved,
    #[serde(rename = "account.created")]
    AccountCreated,
    #[serde(rename = "auth.signed_in")]
    SignedIn,
    #[serde(rename = "auth.sign_in_failed")]
    SignInFailed,
    #[serde(rename = "auth.signed_out")]
    SignedOut,
    #[serde(rename = "auth.password_changed")]
    PasswordChanged,
    #[serde(rename = "auth.password_change_failed")]
    PasswordChangeFailed,
    #[serde(rename = "account.delete_failed")]
    AccountDeleteFailed,
    #[serde(rename = "session.revoked")]
    SessionRevoked,
    #[serde(rename = "token.created")]
    TokenCreated,
    #[serde(rename = "token.revoked")]
    TokenRevoked,
    #[serde(rename = "rate.limited")]
    RateLimited,
}

impl ActivityKind {
    pub const ALL: &'static [Self] = &[
        Self::FileUploaded,
        Self::FileRenamed,
        Self::FileTagged,
        Self::FileDeleted,
        Self::FileOpened,
        Self::SourceAdded,
        Self::SourceRemoved,
        Self::SourceSynced,
        Self::SearchPerformed,
        Self::ChatAsked,
        Self::CollectionCreated,
        Self::CollectionUpdated,
        Self::CollectionDeleted,
        Self::CollectionFilesAdded,
        Self::CollectionFilesRemoved,
        Self::AccountCreated,
        Self::SignedIn,
        Self::SignInFailed,
        Self::SignedOut,
        Self::PasswordChanged,
        Self::PasswordChangeFailed,
        Self::AccountDeleteFailed,
        Self::SessionRevoked,
        Self::TokenCreated,
        Self::TokenRevoked,
        Self::RateLimited,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::FileUploaded => "file.uploaded",
            Self::FileRenamed => "file.renamed",
            Self::FileTagged => "file.tagged",
            Self::FileDeleted => "file.deleted",
            Self::FileOpened => "file.opened",
            Self::SourceAdded => "source.added",
            Self::SourceRemoved => "source.removed",
            Self::SourceSynced => "source.synced",
            Self::SearchPerformed => "search.performed",
            Self::ChatAsked => "chat.asked",
            Self::CollectionCreated => "collection.created",
            Self::CollectionUpdated => "collection.updated",
            Self::CollectionDeleted => "collection.deleted",
            Self::CollectionFilesAdded => "collection.files_added",
            Self::CollectionFilesRemoved => "collection.files_removed",
            Self::AccountCreated => "account.created",
            Self::SignedIn => "auth.signed_in",
            Self::SignInFailed => "auth.sign_in_failed",
            Self::SignedOut => "auth.signed_out",
            Self::PasswordChanged => "auth.password_changed",
            Self::PasswordChangeFailed => "auth.password_change_failed",
            Self::AccountDeleteFailed => "account.delete_failed",
            Self::SessionRevoked => "session.revoked",
            Self::TokenCreated => "token.created",
            Self::TokenRevoked => "token.revoked",
            Self::RateLimited => "rate.limited",
        }
    }

    pub fn category(self) -> ActivityCategory {
        use ActivityCategory as C;
        match self {
            Self::FileUploaded => C::Files,
            Self::FileRenamed => C::Files,
            Self::FileTagged => C::Files,
            Self::FileDeleted => C::Files,
            Self::FileOpened => C::Files,
            Self::SourceAdded => C::Files,
            Self::SourceRemoved => C::Files,
            Self::SourceSynced => C::Files,
            Self::SearchPerformed => C::Search,
            Self::ChatAsked => C::Chat,
            Self::CollectionCreated => C::Collections,
            Self::CollectionUpdated => C::Collections,
            Self::CollectionDeleted => C::Collections,
            Self::CollectionFilesAdded => C::Collections,
            Self::CollectionFilesRemoved => C::Collections,
            Self::AccountCreated => C::Security,
            Self::SignedIn => C::Security,
            Self::SignInFailed => C::Security,
            Self::SignedOut => C::Security,
            Self::PasswordChanged => C::Security,
            Self::PasswordChangeFailed => C::Security,
            Self::AccountDeleteFailed => C::Security,
            Self::SessionRevoked => C::Security,
            Self::TokenCreated => C::Security,
            Self::TokenRevoked => C::Security,
            Self::RateLimited => C::Security,
        }
    }

    /// The kind stored as `name`, if this build knows it.
    pub fn parse(name: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|k| k.as_str() == name)
    }
}

/// Who did it and how they were signed in.
#[derive(Debug, Clone, Copy)]
pub struct Actor {
    pub user_id: Uuid,
    /// `session` (browser) or `token` (API token / MCP).
    pub via: &'static str,
}

impl Actor {
    pub fn session(user_id: Uuid) -> Self {
        Self {
            user_id,
            via: "session",
        }
    }
}

impl From<&AuthUser> for Actor {
    fn from(auth: &AuthUser) -> Self {
        Self {
            user_id: auth.user_id,
            via: match auth.credential {
                Credential::Session { .. } => "session",
                Credential::Token { .. } => "token",
            },
        }
    }
}

impl From<AuthUser> for Actor {
    fn from(auth: AuthUser) -> Self {
        Self::from(&auth)
    }
}

/// A new event of `kind` by `actor` (add subject, links and details to it).
pub fn event<'a>(actor: Actor, kind: ActivityKind) -> NewEvent<'a> {
    let mut ev = NewEvent::new(actor.user_id, kind.as_str(), kind.category().as_str());
    ev.via = Some(actor.via);
    ev
}

/// Record inside the action's transaction.
pub async fn record(conn: &mut PgConnection, ev: &NewEvent<'_>) -> Result<(), sqlx::Error> {
    akasha_db::activity::record(conn, ev).await
}

/// Record outside any transaction; failures are logged, never returned.
pub async fn record_best_effort(db: &PgPool, ev: &NewEvent<'_>) {
    let result = match db.acquire().await {
        Ok(mut conn) => akasha_db::activity::record(&mut conn, ev).await,
        Err(e) => Err(e),
    };
    if let Err(e) = result {
        tracing::warn!(error = %e, kind = ev.kind, "could not record activity");
    }
}

/// Note that a user hit a per-user rate limit (at most once per 10 minutes per
/// user, in the background so the 429 is not delayed).
pub fn rate_limited(db: &PgPool, actor: Actor, what: &'static str) {
    let db = db.clone();
    tokio::spawn(async move {
        let mut ev = event(actor, ActivityKind::RateLimited);
        ev.subject = Some(what);
        let result = match db.acquire().await {
            Ok(mut conn) => akasha_db::activity::record_unless_recent(&mut conn, &ev, 600.0).await,
            Err(e) => Err(e),
        };
        if let Err(e) = result {
            tracing::warn!(error = %e, "could not record a rate-limit event");
        }
    });
}

/// The client's address and user agent, for sign-ins and the audit log. Never
/// fails: either may be missing. The address is the one [`crate::client_ip`]
/// resolved (the client behind a trusted reverse proxy, else the TCP peer).
#[derive(Debug, Clone, Default)]
pub struct ClientMeta {
    pub ip: Option<String>,
    pub user_agent: Option<String>,
    /// A trusted proxy reported HTTPS: session cookies get `Secure`.
    pub https: bool,
}

impl ClientMeta {
    pub fn apply<'a>(&'a self, ev: &mut NewEvent<'a>) {
        ev.ip = self.ip.as_deref();
        ev.user_agent = self.user_agent.as_deref();
    }
}

impl<S: Send + Sync> FromRequestParts<S> for ClientMeta {
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let client = parts.extensions.get::<ClientAddr>().copied();
        let user_agent = parts
            .headers
            .get(USER_AGENT)
            .and_then(|v| v.to_str().ok())
            .map(|ua| ua.chars().take(256).collect());
        Ok(Self {
            ip: client.map(|c| c.ip.to_string()),
            user_agent,
            https: client.is_some_and(|c| c.https),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kinds_round_trip_and_match_the_schema_pattern() {
        for kind in ActivityKind::ALL {
            assert_eq!(ActivityKind::parse(kind.as_str()), Some(*kind));
            let (area, verb) = kind.as_str().split_once('.').expect("area.verb");
            let ok =
                |s: &str| !s.is_empty() && s.chars().all(|c| c.is_ascii_lowercase() || c == '_');
            assert!(ok(area) && ok(verb), "{}", kind.as_str());
        }
        assert_eq!(ActivityKind::parse("nope.never"), None);
    }
}
