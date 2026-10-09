//! Route table and OpenAPI document. Add each new feature as a sub-module and
//! register its paths in [`ApiDoc`].

mod auth;
mod health;
mod me;

use axum::{
    Json, Router,
    routing::{get, post},
};
use utoipa::{
    Modify, OpenApi,
    openapi::security::{ApiKey, ApiKeyValue, SecurityScheme},
};

use crate::{
    auth::session::COOKIE_NAME,
    error::{ApiError, ErrorBody},
    rate_limit,
    state::AppState,
};
use akasha_core::Error;

#[derive(OpenApi)]
#[openapi(
    info(title = "Akasha API", description = "Personal knowledge retrieval."),
    paths(
        health::healthz, health::readyz,
        auth::register, auth::login, auth::logout,
        me::get_me, me::update_me, me::change_password, me::delete_me,
    ),
    components(schemas(ErrorBody, health::Health)),
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

pub fn router(state: &AppState) -> Router<AppState> {
    // Credential-checking endpoints share one per-IP limiter.
    let limited = Router::new()
        .route("/api/v1/auth/register", post(auth::register))
        .route("/api/v1/auth/login", post(auth::login))
        .route("/api/v1/me/password", post(me::change_password))
        .route("/api/v1/me", axum::routing::delete(me::delete_me))
        .layer(rate_limit::layer(&state.auth_limiter));

    Router::new()
        .route("/healthz", get(health::healthz))
        .route("/readyz", get(health::readyz))
        .route(
            "/api/openapi.json",
            get(|| async { Json(ApiDoc::openapi()) }),
        )
        .route("/api/v1/auth/logout", post(auth::logout))
        .route("/api/v1/me", get(me::get_me).patch(me::update_me))
        .merge(limited)
        .fallback(|| async { ApiError(Error::not_found("no such route")) })
}
