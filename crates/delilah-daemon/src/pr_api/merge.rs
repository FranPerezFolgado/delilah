//! `POST /tasks/{key}/prs/{id}/merge`: merges a pull request through `gh`.

use axum::Json;
use axum::extract::{Path, State};

use super::dto::PrView;
use crate::api::ApiError;
use crate::state::AppState;

pub async fn merge(
    State(_app): State<AppState>,
    Path(_path): Path<(String, i64)>,
) -> Result<Json<PrView>, ApiError> {
    Err(ApiError::not_implemented())
}
