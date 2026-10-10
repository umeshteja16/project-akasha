//! Route table and OpenAPI document. Add each new feature as a sub-module and
//! register its paths in [`ApiDoc`].

mod activity;
mod auth;
pub(crate) mod chat;
pub(crate) mod collections;
mod cursor;
pub(crate) mod files;
mod health;
mod me;
mod meta;
pub(crate) mod search;
mod sessions;
mod system;
mod tokens;

use std::time::Duration;

use axum::{
    Json, Router,
    extract::DefaultBodyLimit,
    http::StatusCode,
    routing::{get, post},
};
use tower_http::{compression::CompressionLayer, timeout::TimeoutLayer};
use utoipa::{
    Modify, OpenApi,
    openapi::security::{ApiKey, ApiKeyValue, HttpAuthScheme, HttpBuilder, SecurityScheme},
};

use crate::{auth::session::COOKIE_NAME, error::ErrorBody, rate_limit, state::AppState};

#[derive(OpenApi)]
#[openapi(
    info(title = "Akasha API", description = "Personal knowledge retrieval."),
    paths(
        health::healthz, health::readyz, meta::get_meta,
        auth::register, auth::login, auth::logout,
        me::get_me, me::update_me, me::change_password, me::delete_me,
        files::upload::upload, files::list, files::get, files::update, files::delete,
        files::bulk_delete, files::download::download, files::processing::reindex,
        files::enrich::enrich,
        files::extraction::get, files::similar::similar, files::thumbnail::thumbnail,
        files::tags::list, files::open,
        collections::list, collections::create, collections::get, collections::update,
        collections::delete, collections::add_files, collections::remove_files,
        activity::list, activity::clear,
        sessions::list, sessions::revoke, sessions::revoke_others,
        search::search, search::search_chunks,
        chat::create, chat::list, chat::get, chat::update, chat::delete,
        chat::list_messages, chat::messages::post,
        system::status,
        tokens::list, tokens::create, tokens::revoke,
    ),
    components(schemas(
        ErrorBody, health::Health, files::types::FileCategory, files::types::FileSort,
        crate::chat::events::ChatSources, crate::chat::events::ChatDelta,
        crate::chat::events::ChatDone, crate::chat::events::ChatError,
    )),
    modifiers(&SessionCookie)
)]
pub struct ApiDoc;

struct SessionCookie;

impl Modify for SessionCookie {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        let components = openapi.components.get_or_insert_with(Default::default);
        components.add_security_scheme(
            "session_cookie",
            SecurityScheme::ApiKey(ApiKey::Cookie(ApiKeyValue::new(COOKIE_NAME))),
        );
        // Personal API tokens (`akasha_pat_…`) work wherever the cookie does,
        // except account and token management; read-only tokens only for GET.
        components.add_security_scheme(
            "api_token",
            SecurityScheme::Http(
                HttpBuilder::new()
                    .scheme(HttpAuthScheme::Bearer)
                    .description(Some("Personal API token (akasha_pat_…)"))
                    .build(),
            ),
        );
    }
}

/// Deadline for ordinary requests.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
/// Deadline for file uploads and downloads, which may be large and slow.
const TRANSFER_TIMEOUT: Duration = Duration::from_secs(60 * 60);
/// Room for multipart framing on top of the file itself.
const MULTIPART_OVERHEAD: u64 = 1024 * 1024;

pub fn router(state: &AppState) -> Router<AppState> {
    // Credential-checking endpoints share one per-IP limiter.
    let limited = Router::new()
        .route("/api/v1/auth/register", post(auth::register))
        .route("/api/v1/auth/login", post(auth::login))
        .route("/api/v1/me/password", post(me::change_password))
        .route("/api/v1/me", axum::routing::delete(me::delete_me))
        .layer(rate_limit::layer(&state.auth_limiter));

    let body_limit = state
        .config
        .max_upload_bytes()
        .saturating_add(MULTIPART_OVERHEAD);
    let transfers = Router::new()
        .route("/api/v1/files", post(files::upload::upload))
        .route(
            "/api/v1/files/{id}/download",
            get(files::download::download),
        )
        .layer(DefaultBodyLimit::max(
            usize::try_from(body_limit).unwrap_or(usize::MAX),
        ))
        .layer(TimeoutLayer::with_status_code(
            StatusCode::REQUEST_TIMEOUT,
            TRANSFER_TIMEOUT,
        ));

    Router::new()
        .route("/healthz", get(health::healthz))
        .route("/readyz", get(health::readyz))
        .route(
            "/api/openapi.json",
            get(|| async { Json(ApiDoc::openapi()) }),
        )
        .route("/api/v1/meta", get(meta::get_meta))
        .route("/api/v1/system/status", get(system::status))
        .route("/api/v1/auth/logout", post(auth::logout))
        .route("/api/v1/me", get(me::get_me).patch(me::update_me))
        .route("/api/v1/me/tokens", get(tokens::list).post(tokens::create))
        .route(
            "/api/v1/me/tokens/{id}",
            axum::routing::delete(tokens::revoke),
        )
        .route("/api/v1/me/sessions", get(sessions::list))
        .route(
            "/api/v1/me/sessions/revoke-others",
            post(sessions::revoke_others),
        )
        .route(
            "/api/v1/me/sessions/{id}",
            axum::routing::delete(sessions::revoke),
        )
        .route(
            "/api/v1/activity",
            get(activity::list).delete(activity::clear),
        )
        .route(
            "/api/v1/collections",
            get(collections::list).post(collections::create),
        )
        .route(
            "/api/v1/collections/{id}",
            get(collections::get)
                .patch(collections::update)
                .delete(collections::delete),
        )
        .route(
            "/api/v1/collections/{id}/files",
            post(collections::add_files),
        )
        .route(
            "/api/v1/collections/{id}/files/remove",
            post(collections::remove_files),
        )
        .route("/api/v1/files/{id}/open", post(files::open))
        .route("/api/v1/files", get(files::list))
        .route("/api/v1/files/bulk-delete", post(files::bulk_delete))
        .route("/api/v1/tags", get(files::tags::list))
        .route("/api/v1/files/{id}/extraction", get(files::extraction::get))
        .route("/api/v1/files/{id}/similar", get(files::similar::similar))
        .route(
            "/api/v1/files/{id}/thumbnail",
            get(files::thumbnail::thumbnail),
        )
        .route("/api/v1/search", get(search::search))
        .route("/api/v1/search/chunks", get(search::search_chunks))
        .route("/api/v1/conversations", get(chat::list).post(chat::create))
        .route(
            "/api/v1/conversations/{id}",
            get(chat::get).patch(chat::update).delete(chat::delete),
        )
        .route(
            "/api/v1/conversations/{id}/messages",
            get(chat::list_messages).post(chat::messages::post),
        )
        .route(
            "/api/v1/files/{id}/reindex",
            post(files::processing::reindex),
        )
        .route("/api/v1/files/{id}/enrich", post(files::enrich::enrich))
        .route(
            "/api/v1/files/{id}",
            get(files::get).patch(files::update).delete(files::delete),
        )
        .merge(limited)
        .layer(TimeoutLayer::with_status_code(
            StatusCode::REQUEST_TIMEOUT,
            REQUEST_TIMEOUT,
        ))
        .merge(transfers)
        .merge(crate::mcp::router(state))
        // Unknown `/api` paths are JSON 404s; browser paths get the embedded UI,
        // compressed (gzip or brotli, as the browser accepts).
        .fallback_service(
            Router::new()
                .fallback(crate::web::fallback)
                .layer(CompressionLayer::new())
                .with_state(state.clone()),
        )
}
