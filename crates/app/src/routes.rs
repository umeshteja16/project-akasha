//! Route table and OpenAPI document. Add each new feature as a sub-module and
//! register its paths in [`ApiDoc`].

mod health;

use axum::{Json, Router, routing::get};
use utoipa::OpenApi;

use crate::{
    error::{ApiError, ErrorBody},
    state::AppState,
};
use akasha_core::Error;

#[derive(OpenApi)]
#[openapi(
    info(title = "Akasha API", description = "Personal knowledge retrieval."),
    paths(health::healthz, health::readyz),
    components(schemas(ErrorBody, health::Health))
)]
pub struct ApiDoc;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/healthz", get(health::healthz))
        .route("/readyz", get(health::readyz))
        .route(
            "/api/openapi.json",
            get(|| async { Json(ApiDoc::openapi()) }),
        )
        .fallback(|| async { ApiError(Error::not_found("no such route")) })
}
