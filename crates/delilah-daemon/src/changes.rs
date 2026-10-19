//! What a task changed, for reviewing it in the app: per repo of the workspace (each
//! submodule of a parent), the files the branch touched and the diff of each.

use axum::Json;
use axum::extract::{Path, Query, State};
use delilah_core::TaskKey;
use delilah_git::{Changes, GitOptions, Workspace, changes, file_diff};
use serde::{Deserialize, Serialize};

use crate::api::ApiError;
use crate::state::AppState;

#[derive(Debug, Serialize)]
pub struct RepoChanges {
    /// Path in the workspace: the submodule, or `.` for a plain repo.
    pub repo: String,
    #[serde(flatten)]
    pub changes: Changes,
}

#[derive(Debug, Deserialize)]
pub struct DiffQuery {
    pub repo: String,
    pub path: String,
}

async fn workspace(app: &AppState, key: &TaskKey) -> Result<Workspace, ApiError> {
    let ws = app.store.workspace(key).await?.map(|(_, ws)| ws);
    ws.filter(|ws| ws.path.is_dir())
        .ok_or_else(|| ApiError::not_found(format!("{key} has no workspace")))
}

fn options(app: &AppState) -> GitOptions {
    GitOptions {
        allow_file_protocol: app.config.allow_file_protocol,
    }
}

/// `GET /api/v1/tasks/{key}/changes`
pub async fn list(
    State(app): State<AppState>,
    Path(key): Path<String>,
) -> Result<Json<Vec<RepoChanges>>, ApiError> {
    let ws = workspace(&app, &TaskKey::new(key)).await?;
    let opts = options(&app);
    let repos = tokio::task::spawn_blocking(move || {
        ws.repos()
            .into_iter()
            .map(|(repo, dir)| {
                let changes =
                    changes(&dir, &opts).map_err(|e| ApiError::internal(e.to_string()))?;
                Ok(RepoChanges { repo, changes })
            })
            .collect::<Result<Vec<_>, ApiError>>()
    })
    .await
    .map_err(|e| ApiError::internal(e.to_string()))??;
    Ok(Json(repos))
}

/// `GET /api/v1/tasks/{key}/changes/diff?repo=…&path=…`: the unified diff of one file.
pub async fn diff(
    State(app): State<AppState>,
    Path(key): Path<String>,
    Query(q): Query<DiffQuery>,
) -> Result<String, ApiError> {
    let key = TaskKey::new(key);
    let ws = workspace(&app, &key).await?;
    let opts = options(&app);
    tokio::task::spawn_blocking(move || {
        let missing = || ApiError::not_found(format!("{} has no change to `{}`", q.repo, q.path));
        // Only a repo of this workspace and a file it changed: nothing else is readable.
        let (_, dir) = ws
            .repos()
            .into_iter()
            .find(|(repo, _)| *repo == q.repo)
            .ok_or_else(missing)?;
        let found = changes(&dir, &opts).map_err(|e| ApiError::internal(e.to_string()))?;
        let file = found
            .files
            .iter()
            .find(|f| f.path == q.path)
            .ok_or_else(missing)?;
        file_diff(&dir, file, &opts).map_err(|e| ApiError::internal(e.to_string()))
    })
    .await
    .map_err(|e| ApiError::internal(e.to_string()))?
}
