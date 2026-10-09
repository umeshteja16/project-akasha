//! `GET /api/v1/tags`: the tags on your files, for filter pickers.

use axum::extract::State;
use serde::Serialize;
use utoipa::ToSchema;

use crate::{
    auth::AuthUser,
    error::{ApiError, ErrorBody},
    extract::Json,
    state::AppState,
};

/// A tag and how many of your files carry it.
#[derive(Debug, Serialize, ToSchema)]
pub struct TagSummary {
    pub tag: String,
    /// Files with it as one of your own tags.
    pub user_files: i64,
    /// Files where it is only a model-suggested tag.
    pub auto_files: i64,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct TagList {
    /// Most used first.
    pub items: Vec<TagSummary>,
}

/// Every tag on your files (yours and suggested ones), with counts.
#[utoipa::path(
    get, path = "/api/v1/tags", tag = "files", operation_id = "list_tags",
    responses((status = 200, body = TagList), (status = 401, body = ErrorBody)),
    security(("session_cookie" = []))
)]
pub async fn list(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<TagList>, ApiError> {
    let items = akasha_db::files::tag_counts(&state.db, auth.user_id)
        .await?
        .into_iter()
        .map(|t| TagSummary {
            tag: t.tag,
            user_files: t.user_files,
            auto_files: t.auto_files,
        })
        .collect();
    Ok(Json(TagList { items }))
}
