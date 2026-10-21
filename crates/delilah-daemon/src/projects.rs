//! `GET/POST /api/v1/projects`.

use std::path::PathBuf;

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use delilah_agents::suggest;
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
    let root = checked_root(new.path.clone(), new.kind).await?;

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
    app.emit(crate::events::Topic::Projects);
    Ok((StatusCode::CREATED, Json(project)))
}

/// What can change in a project: everything but its history.
#[derive(Debug, Deserialize)]
pub struct ProjectChange {
    pub name: Option<String>,
    pub path: Option<PathBuf>,
    pub kind: Option<ProjectKind>,
    pub workflow: Option<String>,
}

/// `PATCH /api/v1/projects/{name}`
pub async fn update_project(
    State(app): State<AppState>,
    axum::extract::Path(name): axum::extract::Path<String>,
    Json(change): Json<ProjectChange>,
) -> Result<Json<Project>, ApiError> {
    let old = app.store.project(&name).await?;
    let mut project = old.clone();
    if let Some(new_name) = change
        .name
        .map(|n| n.trim().to_string())
        .filter(|n| !n.is_empty())
    {
        project.name = new_name;
    }
    if let Some(workflow) = change.workflow.filter(|w| !w.trim().is_empty()) {
        project.workflow = workflow;
    }
    if let Some(kind) = change.kind {
        project.kind = kind;
    }
    if let Some(path) = change.path {
        project.path = path;
    }
    if project.path != old.path
        && let Some((task, false)) = app.store.checkout_user(&old.path).await?
    {
        return Err(ApiError::conflict(format!(
            "{task} is working in this checkout: archive it before moving the project"
        )));
    }
    if project.path != old.path || project.kind != old.kind {
        project.path = checked_root(project.path.clone(), project.kind).await?;
    }
    app.store.update_project(&name, &project).await?;
    app.emit(crate::events::Topic::Projects);
    app.emit(crate::events::Topic::Tasks);
    app.emit(crate::events::Topic::Sessions);
    Ok(Json(project))
}

/// `DELETE /api/v1/projects/{name}`: refused while a task of it has a workspace, so no
/// work is lost; its own sessions stop and go, its finished tasks stay.
pub async fn delete_project(
    State(app): State<AppState>,
    axum::extract::Path(name): axum::extract::Path<String>,
) -> Result<StatusCode, ApiError> {
    app.store.project(&name).await?;
    if let Some(task) = app.store.project_in_use(&name).await? {
        return Err(ApiError::conflict(format!(
            "{task} still has a workspace: mark it done and archive it first"
        )));
    }
    let owner = crate::store_sessions::project_owner(&name);
    for live in app.sessions.of_task(&owner) {
        let _ = live.pty.kill();
    }
    app.store.delete_project(&name).await?;
    for topic in [
        crate::events::Topic::Projects,
        crate::events::Topic::Tasks,
        crate::events::Topic::Sessions,
    ] {
        app.emit(topic);
    }
    Ok(StatusCode::NO_CONTENT)
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

#[derive(Debug, Deserialize)]
pub struct SuggestRequest {
    pub key: String,
    pub title: String,
    #[serde(default)]
    pub components: Vec<String>,
    /// Defaults to the Jira ticket's description, when Jira has it.
    pub description: Option<String>,
}

#[derive(Debug, serde::Serialize)]
pub struct SuggestResponse {
    pub submodules: Vec<suggest::Suggestion>,
    pub model: &'static str,
}

/// Asks Claude, read-only and on a small model, which submodules a ticket touches.
pub async fn suggest_submodules(
    State(app): State<AppState>,
    axum::extract::Path(name): axum::extract::Path<String>,
    Json(req): Json<SuggestRequest>,
) -> Result<Json<SuggestResponse>, ApiError> {
    let project = app.store.project(&name).await?;
    if project.kind != ProjectKind::Parent {
        return Err(ApiError::bad_request(format!(
            "{name} is a repo project: it has no submodules"
        )));
    }
    let root = project.path.clone();
    let known: Vec<String> =
        tokio::task::spawn_blocking(move || delilah_git::list_submodules(&root))
            .await
            .map_err(|e| ApiError::internal(e.to_string()))?
            .map_err(|e| ApiError::unprocessable(e.to_string()))?
            .into_iter()
            .map(|s| s.path)
            .collect();
    let description = match req.description {
        Some(d) => d,
        None => crate::jira::fetch_issue(&app, &req.key)
            .await
            .map(|i| i.description)
            .unwrap_or_default(),
    };
    let prompt = suggest::prompt(
        &suggest::Ticket {
            key: &req.key,
            title: &req.title,
            components: &req.components,
            description: &description,
        },
        &known,
        app.lang().name(),
    );

    let mut cmd = tokio::process::Command::new("claude");
    cmd.env_clear()
        .envs(suggest::env())
        .args(suggest::args(&prompt))
        .current_dir(&project.path)
        .stdin(std::process::Stdio::null())
        .kill_on_drop(true);
    let output = tokio::time::timeout(SUGGEST_TIMEOUT, cmd.output())
        .await
        .map_err(|_| {
            ApiError::unprocessable("Claude took more than 2 minutes; pick the submodules by hand")
        })?
        .map_err(|e| ApiError::internal(format!("cannot run claude: {e}")))?;
    let submodules = suggest::parse(&String::from_utf8_lossy(&output.stdout), &known)
        .map_err(ApiError::unprocessable)?;
    tracing::info!(task = %req.key, suggested = submodules.len(), "suggested submodules");
    Ok(Json(SuggestResponse {
        submodules,
        model: suggest::MODEL,
    }))
}

const SUGGEST_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(120);

#[derive(Debug, Deserialize)]
pub struct InspectRequest {
    pub path: PathBuf,
}

/// What a folder would be as a project, to prefill the "add project" form.
#[derive(Debug, serde::Serialize)]
pub struct Inspection {
    /// Root of the repo containing the folder.
    pub path: PathBuf,
    pub name: String,
    pub kind: ProjectKind,
    pub submodules: usize,
}

pub async fn inspect(Json(req): Json<InspectRequest>) -> Result<Json<Inspection>, ApiError> {
    let path = req.path.clone();
    let inspection = tokio::task::spawn_blocking(move || -> Result<Inspection, String> {
        let root = delilah_git::repo_root(&path)
            .map_err(|_| format!("{} is not a git repo", path.display()))?;
        let submodules = delilah_git::list_submodules(&root)
            .map_err(|e| e.to_string())?
            .len();
        let name = root
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        Ok(Inspection {
            kind: if submodules > 0 {
                ProjectKind::Parent
            } else {
                ProjectKind::Repo
            },
            path: root,
            name,
            submodules,
        })
    })
    .await
    .map_err(|e| ApiError::internal(e.to_string()))?
    .map_err(ApiError::unprocessable)?;
    Ok(Json(inspection))
}

/// The root of the repo at `path`; a `parent` one must actually have submodules.
async fn checked_root(path: PathBuf, kind: ProjectKind) -> Result<PathBuf, ApiError> {
    tokio::task::spawn_blocking(move || -> Result<PathBuf, String> {
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
    .map_err(ApiError::unprocessable)
}

#[cfg(test)]
mod tests {
    use std::process::Command;

    use axum::body::Body;
    use axum::http::Request;
    use serde_json::{Value, json};
    use tower::ServiceExt;

    use crate::config::Config;
    use crate::state::AppState;
    use crate::store::Store;

    async fn inspect(path: &std::path::Path) -> (axum::http::StatusCode, Value) {
        let config = Config {
            home: std::env::temp_dir(),
            addr: crate::config::DEFAULT_ADDR.parse().unwrap(),
            allow_file_protocol: false,
            notify: false,
            ui_origin: None,
            agent_program: None,
        };
        let app = crate::api::router(AppState::new(Store::in_memory().await.unwrap(), config));
        let req = Request::post("/api/v1/projects/inspect")
            .header("content-type", "application/json")
            .body(Body::from(json!({ "path": path }).to_string()))
            .unwrap();
        let res = app.oneshot(req).await.unwrap();
        let status = res.status();
        let bytes = http_body_util::BodyExt::collect(res.into_body())
            .await
            .unwrap()
            .to_bytes();
        (
            status,
            serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        )
    }

    #[tokio::test]
    async fn proposes_kind_and_name_from_the_folder() {
        let dir = std::env::temp_dir().join(format!("delilah-inspect-{}", uuid::Uuid::new_v4()));
        let fixture = Command::new("bash")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../scripts/make-fixture-parent.sh"
            ))
            .arg(dir.join("env"))
            .output()
            .unwrap();
        assert!(fixture.status.success());

        // A subfolder of the parent: the project is its root, with submodules.
        let (_, parent) = inspect(&dir.join("env/parent/svc-payments/..")).await;
        assert_eq!(parent["kind"], "parent");
        assert_eq!(parent["name"], "parent");
        assert_eq!(parent["submodules"], 4);

        let (_, repo) = inspect(&dir.join("env/parent/svc-orders/src")).await;
        assert_eq!(repo["kind"], "repo");
        assert_eq!(repo["name"], "svc-orders");

        let (status, body) = inspect(&dir).await;
        assert_eq!(status, axum::http::StatusCode::UNPROCESSABLE_ENTITY);
        assert!(body["error"].as_str().unwrap().contains("not a git repo"));
        std::fs::remove_dir_all(dir).unwrap();
    }
}
