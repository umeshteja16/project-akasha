use axum::{Json, extract::State};
use serde::Serialize;
use utoipa::ToSchema;

use crate::{error::ApiError, state::AppState};
use akasha_core::Error;

#[derive(Serialize, ToSchema)]
pub struct Health {
    pub status: &'static str,
    pub version: &'static str,
}

const OK: Health = Health {
    status: "ok",
    version: env!("CARGO_PKG_VERSION"),
};

/// Liveness: the process is up. Never touches dependencies.
#[utoipa::path(get, path = "/healthz", tag = "ops", responses((status = 200, body = Health)))]
pub async fn healthz() -> Json<Health> {
    Json(OK)
}

/// Readiness: the process can serve traffic (database reachable).
#[utoipa::path(
    get, path = "/readyz", tag = "ops",
    responses((status = 200, body = Health), (status = 503, body = crate::error::ErrorBody))
)]
pub async fn readyz(State(state): State<AppState>) -> Result<Json<Health>, ApiError> {
    akasha_db::ping(&state.db).await.map_err(|err| {
        tracing::warn!(%err, "readiness check failed");
        Error::unavailable("database unreachable")
    })?;
    Ok(Json(OK))
}
