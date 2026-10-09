//! Serving the single-page web UI from the binary (production is one process:
//! UI + API). In development Vite serves the UI and proxies `/api` here.
//!
//! - `/api/*` never falls back to the UI: unknown API paths stay JSON 404s.
//! - Hashed build output (`assets/*`) is cached for a year (`immutable`);
//!   everything else, `index.html` first, is revalidated on every load (ETag).
//! - Any other GET for a path without a file extension gets `index.html`, so
//!   client-side routes survive a reload (SPA fallback).
//! - Every response carries the security headers from [`security_headers`].

mod assets;

use std::borrow::Cow;

use axum::{
    Router,
    body::Body,
    extract::State,
    http::{HeaderMap, HeaderName, HeaderValue, Method, StatusCode, Uri, header},
    response::{IntoResponse, Response},
};
use bytes::Bytes;
use tower_http::set_header::SetResponseHeaderLayer;

pub use assets::{Asset, WebAssets};

use crate::{error::ApiError, state::AppState};
use akasha_core::Error;

/// Policy for the SPA. Styles allow `'unsafe-inline'` because the UI primitives
/// (Radix scroll locking, positioning) inject `<style>` elements; scripts stay
/// strictly same-origin (no inline scripts in `index.html`).
pub const CONTENT_SECURITY_POLICY: &str = "default-src 'self'; script-src 'self'; \
     style-src 'self' 'unsafe-inline'; img-src 'self' data: blob:; font-src 'self' data:; \
     connect-src 'self'; media-src 'self' blob:; object-src 'none'; base-uri 'self'; \
     form-action 'self'; frame-ancestors 'none'";

const IMMUTABLE: &str = "public, max-age=31536000, immutable";
const REVALIDATE: &str = "no-cache";

/// Router fallback: the UI for browser paths, JSON 404 for everything else.
pub async fn fallback(
    State(state): State<AppState>,
    method: Method,
    uri: Uri,
    headers: HeaderMap,
) -> Response {
    let path = uri.path();
    let is_api = path == "/api" || path.starts_with("/api/");
    if is_api || !(method == Method::GET || method == Method::HEAD) {
        return not_found();
    }
    serve(&state.web, path, &headers).unwrap_or_else(not_found)
}

fn not_found() -> Response {
    ApiError(Error::not_found("no such route")).into_response()
}

fn serve(assets: &WebAssets, path: &str, headers: &HeaderMap) -> Option<Response> {
    let rel = path.trim_start_matches('/');
    let rel = if rel.is_empty() { "index.html" } else { rel };
    if rel.split('/').any(|seg| seg == ".." || seg == ".") {
        return None;
    }
    if let Some(asset) = assets.get(rel) {
        let cache = if rel.starts_with("assets/") {
            IMMUTABLE
        } else {
            REVALIDATE
        };
        return Some(respond(rel, asset, cache, headers));
    }
    // A missing file (it has an extension) is a 404, not the app: a stale
    // chunk must fail loudly instead of parsing HTML as JavaScript.
    let last = rel.rsplit('/').next().unwrap_or(rel);
    if last.contains('.') {
        return None;
    }
    let index = assets.get("index.html")?;
    Some(respond("index.html", index, REVALIDATE, headers))
}

fn respond(path: &str, asset: Asset, cache: &'static str, headers: &HeaderMap) -> Response {
    let etag = HeaderValue::from_str(&asset.etag).ok();
    let not_modified = etag.as_ref().is_some_and(|etag| {
        headers
            .get(header::IF_NONE_MATCH)
            .and_then(|v| v.to_str().ok())
            .is_some_and(|v| v.split(',').any(|tag| tag.trim() == etag))
    });
    let mut response = if not_modified {
        StatusCode::NOT_MODIFIED.into_response()
    } else {
        let mime = mime_guess::from_path(path).first_or_octet_stream();
        let body = match asset.bytes {
            Cow::Borrowed(bytes) => Body::from(Bytes::from_static(bytes)),
            Cow::Owned(bytes) => Body::from(bytes),
        };
        let mut response = Response::new(body);
        if let Ok(value) = HeaderValue::from_str(mime.as_ref()) {
            response.headers_mut().insert(header::CONTENT_TYPE, value);
        }
        response
    };
    let out = response.headers_mut();
    out.insert(header::CACHE_CONTROL, HeaderValue::from_static(cache));
    if let Some(etag) = etag {
        out.insert(header::ETAG, etag);
    }
    response
}

/// Security headers on every response (handlers may set their own first,
/// e.g. downloads use a stricter sandboxing policy).
pub fn security_headers(router: Router) -> Router {
    const HEADERS: [(HeaderName, &str); 5] = [
        (header::CONTENT_SECURITY_POLICY, CONTENT_SECURITY_POLICY),
        (header::X_FRAME_OPTIONS, "DENY"),
        (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
        (header::REFERRER_POLICY, "same-origin"),
        (
            HeaderName::from_static("cross-origin-opener-policy"),
            "same-origin",
        ),
    ];
    HEADERS.into_iter().fold(router, |router, (name, value)| {
        router.layer(SetResponseHeaderLayer::if_not_present(
            name,
            HeaderValue::from_static(value),
        ))
    })
}
