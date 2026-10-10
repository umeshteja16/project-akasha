//! MCP server (ADR 0015): Akasha's search, reading and notes as tools for AI
//! agents, over the Streamable HTTP transport at `/mcp`.
//!
//! - **Auth**: every request carries a personal API token
//!   (`Authorization: Bearer akasha_pat_…`); cookies are not accepted here. The
//!   middleware resolves it to an [`AuthUser`] that the handler reads from the
//!   request; every tool filters by that owner. Read-only tokens do not see the
//!   write tools and cannot call them.
//! - **Stateless**: no MCP sessions (`legacy_session_mode = false`), plain JSON
//!   responses; each POST is one JSON-RPC message, so any number of server
//!   processes can serve it and nothing is kept between requests.
//! - **stdio**: `akasha mcp` ([`bridge`]) forwards stdio JSON-RPC to a running
//!   server's `/mcp`, for clients that only launch local processes.

mod ask;
pub mod bridge;
mod collections;
mod output;
mod read;
mod server;
mod tools;
mod write;

use std::{sync::Arc, time::Duration};

use axum::{
    Router,
    extract::{Request, State},
    http::{HeaderValue, StatusCode, header::WWW_AUTHENTICATE},
    middleware::{self, Next},
    response::{IntoResponse, Response},
};
use rmcp::transport::{
    StreamableHttpServerConfig, StreamableHttpService,
    streamable_http_server::session::never::NeverSessionManager,
};
use tower_http::timeout::TimeoutLayer;
use uuid::Uuid;

use crate::{
    auth::{AuthUser, token},
    error::ApiError,
    state::AppState,
};
use akasha_core::Error;

pub use server::AkashaMcp;

/// Mount path of the Streamable HTTP endpoint.
pub const PATH: &str = "/mcp";
/// Largest JSON-RPC message accepted (a note of 200k characters fits).
const MAX_BODY: usize = 2 * 1024 * 1024;
/// `ask` may wait for a slow model.
const TIMEOUT: Duration = Duration::from_secs(150);

/// Who is calling a tool.
#[derive(Debug, Clone, Copy)]
pub struct Caller {
    pub user_id: Uuid,
    pub can_write: bool,
}

impl Caller {
    /// MCP clients always sign in with an API token.
    pub fn actor(&self) -> crate::activity::Actor {
        crate::activity::Actor {
            user_id: self.user_id,
            via: "token",
        }
    }
}

/// `/mcp`, authenticated with API tokens.
pub fn router(state: &AppState) -> Router<AppState> {
    // Host/Origin checks guard unauthenticated local servers against DNS
    // rebinding; every request here needs a secret bearer token a browser page
    // cannot know, so they are off (the server is reachable under any name).
    let config = StreamableHttpServerConfig::default()
        .with_legacy_session_mode(false)
        .with_json_response(true)
        .with_sse_keep_alive(None)
        .with_max_request_body_bytes(MAX_BODY)
        .disable_allowed_hosts();
    let handler = AkashaMcp {
        state: state.clone(),
    };
    let service = StreamableHttpService::new(
        move || Ok(handler.clone()),
        Arc::new(NeverSessionManager::default()),
        config,
    );
    Router::new()
        .route_service(PATH, service)
        .layer(middleware::from_fn_with_state(state.clone(), authenticate))
        .layer(TimeoutLayer::with_status_code(
            StatusCode::REQUEST_TIMEOUT,
            TIMEOUT,
        ))
}

/// Require a valid API token and hand the user to the MCP handler.
async fn authenticate(State(state): State<AppState>, mut req: Request, next: Next) -> Response {
    let user = match token::bearer(req.headers()) {
        Ok(Some(secret)) => token::authenticate(&state, secret).await,
        Ok(None) => Err(Error::unauthorized(
            "the MCP endpoint needs `Authorization: Bearer <API token>` (create one in Settings → Access tokens)",
        )),
        Err(err) => Err(err),
    };
    match user {
        Ok(grant) => {
            req.extensions_mut().insert(AuthUser::from_grant(grant));
            next.run(req).await
        }
        Err(err) => {
            let mut res = ApiError(err).into_response();
            if res.status() == StatusCode::UNAUTHORIZED {
                res.headers_mut().insert(
                    WWW_AUTHENTICATE,
                    HeaderValue::from_static("Bearer realm=\"akasha\""),
                );
            }
            res
        }
    }
}
