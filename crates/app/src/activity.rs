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

use std::net::{IpAddr, SocketAddr};

use axum::{
    Extension,
    extract::{ConnectInfo, FromRequestParts},
    http::{header::USER_AGENT, request::Parts},
};
use serde::{Deserialize, Serialize};
use sqlx::PgConnection;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::auth::{AuthUser, Credential};
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

macro_rules! kinds {
    ($($variant:ident = $name:literal, $category:ident;)*) => {
        /// What happened.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
        pub enum ActivityKind {
            $(#[serde(rename = $name)] $variant,)*
        }

        impl ActivityKind {
            pub const ALL: &'static [Self] = &[$(Self::$variant,)*];

            pub fn as_str(self) -> &'static str {
                match self { $(Self::$variant => $name,)* }
            }

            pub fn category(self) -> ActivityCategory {
                match self { $(Self::$variant => ActivityCategory::$category,)* }
            }

            /// The kind stored as `name`, if this build knows it.
            pub fn parse(name: &str) -> Option<Self> {
                match name { $($name => Some(Self::$variant),)* _ => None }
            }
        }
    };
}

kinds! {
    FileUploaded = "file.uploaded", Files;
    FileRenamed = "file.renamed", Files;
    FileTagged = "file.tagged", Files;
    FileDeleted = "file.deleted", Files;
    FileOpened = "file.opened", Files;
    SearchPerformed = "search.performed", Search;
    ChatAsked = "chat.asked", Chat;
    CollectionCreated = "collection.created", Collections;
    CollectionUpdated = "collection.updated", Collections;
    CollectionDeleted = "collection.deleted", Collections;
    CollectionFilesAdded = "collection.files_added", Collections;
    CollectionFilesRemoved = "collection.files_removed", Collections;
    AccountCreated = "account.created", Security;
    SignedIn = "auth.signed_in", Security;
    SignInFailed = "auth.sign_in_failed", Security;
    SignedOut = "auth.signed_out", Security;
    PasswordChanged = "auth.password_changed", Security;
    PasswordChangeFailed = "auth.password_change_failed", Security;
    AccountDeleteFailed = "account.delete_failed", Security;
    SessionRevoked = "session.revoked", Security;
    TokenCreated = "token.created", Security;
    TokenRevoked = "token.revoked", Security;
    RateLimited = "rate.limited", Security;
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
/// fails: either may be missing. The address is the TCP peer (no proxy headers
/// are trusted yet; see PROGRESS.md).
#[derive(Debug, Clone, Default)]
pub struct ClientMeta {
    pub ip: Option<String>,
    pub user_agent: Option<String>,
}

impl ClientMeta {
    pub fn apply<'a>(&'a self, ev: &mut NewEvent<'a>) {
        ev.ip = self.ip.as_deref();
        ev.user_agent = self.user_agent.as_deref();
    }
}

impl<S: Send + Sync> FromRequestParts<S> for ClientMeta {
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let peer = Option::<Extension<ConnectInfo<SocketAddr>>>::from_request_parts(parts, state)
            .await
            .ok()
            .flatten()
            .map(|Extension(ConnectInfo(addr))| canonical(addr.ip()).to_string());
        let user_agent = parts
            .headers
            .get(USER_AGENT)
            .and_then(|v| v.to_str().ok())
            .map(|ua| ua.chars().take(256).collect());
        Ok(Self {
            ip: peer,
            user_agent,
        })
    }
}

/// `::ffff:1.2.3.4` → `1.2.3.4`.
fn canonical(ip: IpAddr) -> IpAddr {
    match ip {
        IpAddr::V6(v6) => v6.to_ipv4_mapped().map_or(IpAddr::V6(v6), IpAddr::V4),
        v4 => v4,
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
        assert_eq!(
            canonical("::ffff:10.0.0.1".parse().expect("ip")).to_string(),
            "10.0.0.1"
        );
    }
}
