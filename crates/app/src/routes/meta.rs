//! `/api/v1/meta`: public facts about this server that the UI needs before sign-in.

use axum::{Json, extract::State};
use serde::Serialize;
use utoipa::ToSchema;

use crate::state::AppState;

#[derive(Serialize, ToSchema)]
pub struct ServerMeta {
    /// Server version (the `akasha` crate version).
    pub version: &'static str,
    /// Whether `POST /api/v1/auth/register` accepts new accounts.
    pub allow_registration: bool,
    /// Whether a language model is configured (chat answers and enrichment).
    /// Without one, chat answers with passages only.
    pub chat_model: bool,
    /// Largest file `POST /api/v1/files` accepts, in bytes.
    pub max_upload_bytes: u64,
}

/// Public server facts (no sign-in needed). Reveals nothing about accounts.
#[utoipa::path(
    get, path = "/api/v1/meta", tag = "meta",
    responses((status = 200, body = ServerMeta))
)]
pub async fn get_meta(State(state): State<AppState>) -> Json<ServerMeta> {
    Json(ServerMeta {
        version: env!("CARGO_PKG_VERSION"),
        allow_registration: state.config.allow_registration,
        chat_model: state.llm.is_some(),
        max_upload_bytes: state.config.max_upload_bytes(),
    })
}
