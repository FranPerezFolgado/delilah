//! Who a session, a file tree or a changeset belongs to: a task (its workspace) or a
//! project, as `@<name>` (the project's own checkout, to ask about the code without a
//! task).

use delilah_core::{ProjectKind, TaskKey};
use delilah_git::Workspace;

use crate::api::ApiError;
use crate::state::AppState;
use crate::store_sessions::project_of;

/// The folder an owner works in, as a workspace. A project's lists the submodules
/// initialised in its checkout.
pub async fn workspace(app: &AppState, owner: &str) -> Result<Workspace, ApiError> {
    if let Some(name) = project_of(owner) {
        let project = app.store.project(name).await?;
        let path = project.path.clone();
        let submodules = match project.kind {
            ProjectKind::Repo => Vec::new(),
            ProjectKind::Parent => tokio::task::spawn_blocking(move || {
                delilah_git::list_submodules(&path)
                    .unwrap_or_default()
                    .into_iter()
                    .filter(|s| path.join(&s.path).join(".git").exists())
                    .map(|s| s.path)
                    .collect()
            })
            .await
            .map_err(|e| ApiError::internal(e.to_string()))?,
        };
        return Ok(Workspace {
            path: project.path,
            branch: String::new(),
            kind: project.kind,
            submodules,
        });
    }
    let key = TaskKey::new(owner);
    app.store
        .workspace(&key)
        .await?
        .map(|(_, ws)| ws)
        .filter(|ws| ws.path.is_dir())
        .ok_or_else(|| ApiError::not_found(format!("{key} has no workspace")))
}
