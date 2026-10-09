//! Request extractors whose rejections use the standard error shape.

use axum::{
    extract::{FromRequest, Request, rejection::JsonRejection},
    response::{IntoResponse, Response},
};
use serde::de::DeserializeOwned;

use crate::error::ApiError;
use akasha_core::Error;

/// Like [`axum::Json`], but a malformed body becomes `400 bad_request` in our error shape.
/// Requiring `Content-Type: application/json` also means plain HTML forms on other sites
/// cannot submit to these endpoints.
pub struct Json<T>(pub T);

impl<S, T> FromRequest<S> for Json<T>
where
    T: DeserializeOwned,
    S: Send + Sync,
{
    type Rejection = ApiError;

    async fn from_request(req: Request, state: &S) -> Result<Self, ApiError> {
        axum::Json::<T>::from_request(req, state)
            .await
            .map(|axum::Json(value)| Self(value))
            .map_err(|rejection: JsonRejection| ApiError(Error::bad_request(rejection.body_text())))
    }
}

impl<T: serde::Serialize> IntoResponse for Json<T> {
    fn into_response(self) -> Response {
        axum::Json(self.0).into_response()
    }
}
