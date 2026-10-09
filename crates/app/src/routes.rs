//! Route table and OpenAPI document. Add each new feature as a sub-module and
//! register its paths in [`ApiDoc`].

mod auth;
pub(crate) mod chat;
pub(crate) mod files;
mod health;
mod me;
mod meta;
pub(crate) mod search;

use std::time::Duration;

use axum::{
    Json, Router,
    extract::DefaultBodyLimit,
    http::StatusCode,
    routing::{get, post},
};
use tower_http::timeout::TimeoutLayer;
use utoipa::{
    Modify, OpenApi,
    openapi::security::{ApiKey, ApiKeyValue, SecurityScheme},
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
        search::search, search::search_chunks,
        chat::create, chat::list, chat::get, chat::update, chat::delete,
        chat::list_messages, chat::messages::post,
    ),
    components(schemas(
        ErrorBody, health::Health, files::types::FileCategory,
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
        .route("/api/v1/auth/logout", post(auth::logout))
        .route("/api/v1/me", get(me::get_me).patch(me::update_me))
        .route("/api/v1/files", get(files::list))
        .route("/api/v1/files/bulk-delete", post(files::bulk_delete))
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
        // Unknown `/api` paths are JSON 404s; browser paths get the embedded UI.
        .fallback(crate::web::fallback)
}
