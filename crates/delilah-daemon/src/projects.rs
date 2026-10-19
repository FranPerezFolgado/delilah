//! `GET/POST /api/v1/projects`.

use std::path::PathBuf;

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use delilah_core::{Project, ProjectKind};
use serde::Deserialize;

use crate::api::ApiError;
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct NewProject {
    pub path: PathBuf,
    pub kind: ProjectKind,
    /// Defaults to the repo's folder name.
    pub name: Option<String>,
    #[serde(default = "default_workflow")]
    pub workflow: String,
}

fn default_workflow() -> String {
    "feature".into()
}

pub async fn list_projects(State(app): State<AppState>) -> Result<Json<Vec<Project>>, ApiError> {
    Ok(Json(app.store.list_projects().await?))
}

pub async fn add_project(
    State(app): State<AppState>,
    Json(new): Json<NewProject>,
) -> Result<(StatusCode, Json<Project>), ApiError> {
    let path = new.path.clone();
    let kind = new.kind;
    // A project is the root of a repo; a `parent` one must actually have submodules.
    let root = tokio::task::spawn_blocking(move || -> Result<PathBuf, String> {
        let root = delilah_git::repo_root(&path)
            .map_err(|_| format!("{} is not a git repo", path.display()))?;
        if kind == ProjectKind::Parent
            && delilah_git::list_submodules(&root)
                .map_err(|e| e.to_string())?
                .is_empty()
        {
            return Err(format!(
                "{} has no submodules; add it with --kind repo",
                root.display()
            ));
        }
        Ok(root)
    })
    .await
    .map_err(|e| ApiError::internal(e.to_string()))?
    .map_err(ApiError::unprocessable)?;

    let name = match new.name {
        Some(name) => name,
        None => root
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .ok_or_else(|| ApiError::bad_request("give the project a --name"))?,
    };
    let project = Project {
        name,
        path: root,
        kind: new.kind,
        workflow: new.workflow,
    };
    app.store.create_project(&project).await?;
    Ok((StatusCode::CREATED, Json(project)))
}

/// The submodules a `parent` project offers, for the "new task" form.
pub async fn submodules(
    State(app): State<AppState>,
    axum::extract::Path(name): axum::extract::Path<String>,
) -> Result<Json<Vec<delilah_git::Submodule>>, ApiError> {
    let project = app.store.project(&name).await?;
    if project.kind != ProjectKind::Parent {
        return Ok(Json(Vec::new()));
    }
    let subs = tokio::task::spawn_blocking(move || delilah_git::list_submodules(&project.path))
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?
        .map_err(|e| ApiError::unprocessable(e.to_string()))?;
    Ok(Json(subs))
}
