//! HTTP API of the daemon. Every client (CLI, desktop app, mobile web) goes through
//! it: nothing else reads the database.

use axum::extract::{Path, State};
use axum::http::{HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::Utc;
use delilah_core::{Event, Task, TaskKey};
use serde::{Deserialize, Serialize};
use tower_http::cors::{AllowOrigin, Any, CorsLayer};

use crate::state::AppState;
use crate::store::{EventRecord, Store, StoreError};
use crate::{
    actions, asks, changes, events, files, hooks, inbox, jira, preferences, projects, run,
    session_api, specs, terminal,
};

/// Origins of the desktop app (Tauri) and of its dev server. Nothing else may call the
/// API from a browser: JSON requests need a preflight that other origins fail.
const APP_ORIGINS: &[&str] = &[
    "tauri://localhost",
    "http://tauri.localhost",
    "http://localhost:1420",
];

pub fn router(state: AppState) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(AllowOrigin::list(
            APP_ORIGINS.iter().map(|o| HeaderValue::from_static(o)),
        ))
        .allow_methods(Any)
        .allow_headers(Any);
    Router::new()
        .route("/api/v1/health", get(health))
        .route("/api/v1/tasks", get(list_tasks).post(create_task))
        .route("/api/v1/tasks/{key}", get(get_task))
        .route(
            "/api/v1/tasks/{key}/events",
            get(list_events).post(post_event),
        )
        .route("/api/v1/tasks/{key}/terminal", get(terminal::task_terminal))
        .route(
            "/api/v1/tasks/{key}/sessions",
            get(session_api::list_for_task).post(session_api::open),
        )
        .route("/api/v1/sessions", get(session_api::list_all))
        .route(
            "/api/v1/preferences/{key}",
            get(preferences::get).put(preferences::put),
        )
        .route(
            "/api/v1/sessions/{id}/terminal",
            get(terminal::session_terminal),
        )
        .route("/api/v1/sessions/{id}/close", post(session_api::close))
        .route("/api/v1/sessions/{id}/reopen", post(session_api::reopen))
        .route(
            "/api/v1/sessions/{id}/allow-edits",
            post(session_api::allow_edits),
        )
        .route(
            "/api/v1/projects/{name}/sessions",
            post(session_api::open_in_project),
        )
        .route("/api/v1/tasks/{key}/resume", post(actions::resume))
        .route("/api/v1/tasks/{key}/done", post(actions::done))
        .route("/api/v1/tasks/{key}/archive", post(actions::archive))
        .route("/api/v1/usage", get(actions::usage))
        .route("/api/v1/events", get(events::stream))
        .route("/api/v1/logs", get(crate::logs::tail))
        .route("/api/v1/specs", get(specs::list))
        .route("/api/v1/inbox", get(inbox::list))
        .route("/api/v1/sessions/{id}/reply", post(inbox::reply))
        .route("/api/v1/tasks/{key}/changes", get(changes::list))
        .route("/api/v1/tasks/{key}/changes/diff", get(changes::diff))
        .route("/api/v1/tasks/{key}/tree", get(files::tree))
        .route("/api/v1/tasks/{key}/files", get(files::all))
        .route("/api/v1/tasks/{key}/file", get(files::get).put(files::put))
        .route("/api/v1/tasks/{key}/spec", get(specs::of_task))
        .route("/api/v1/tasks/{key}/spec/files/{name}", get(specs::file))
        .route("/api/v1/jira", get(jira::status).put(jira::connect))
        .route("/api/v1/jira/issues", get(jira::issues))
        .route("/api/v1/jira/issues/{key}", get(jira::issue))
        .route(
            "/api/v1/projects",
            get(projects::list_projects).post(projects::add_project),
        )
        .route("/api/v1/projects/inspect", post(projects::inspect))
        .route(
            "/api/v1/projects/{name}",
            axum::routing::patch(projects::update_project).delete(projects::delete_project),
        )
        .route(
            "/api/v1/projects/{name}/submodules",
            get(projects::submodules),
        )
        .route(
            "/api/v1/projects/{name}/suggest-submodules",
            post(projects::suggest_submodules),
        )
        .route("/api/v1/runs", post(run::start_run))
        .route("/hooks/claude", post(hooks::claude_hook))
        .route("/hooks/claude/statusline", post(hooks::claude_statusline))
        .route("/hooks/claude/ask", post(asks::hook))
        .route("/api/v1/asks/{id}/answer", post(asks::answer))
        .route("/api/v1/asks/{id}/release", post(asks::release))
        .with_state(state)
        .layer(cors)
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Health {
    pub version: String,
    /// The binary this daemon runs and its modification time (Unix seconds) when it
    /// started: the app compares them with the `delilahd` it bundles and restarts the
    /// daemon after an update.
    #[serde(default)]
    pub exe: Option<String>,
    #[serde(default)]
    pub exe_mtime: Option<u64>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct NewTask {
    pub key: String,
    pub title: String,
    #[serde(default = "default_workflow")]
    pub workflow: String,
}

fn default_workflow() -> String {
    "feature".into()
}

async fn health() -> Json<Health> {
    let (exe, exe_mtime) = EXE.get_or_init(running_binary).clone();
    Json(Health {
        version: env!("CARGO_PKG_VERSION").into(),
        exe,
        exe_mtime,
    })
}

static EXE: std::sync::OnceLock<(Option<String>, Option<u64>)> = std::sync::OnceLock::new();

/// Records which binary is running. Call it first thing, before an update can replace it.
pub fn health_init() {
    EXE.get_or_init(running_binary);
}

fn running_binary() -> (Option<String>, Option<u64>) {
    let Ok(exe) = std::env::current_exe() else {
        return (None, None);
    };
    let mtime = std::fs::metadata(&exe)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs());
    (Some(exe.display().to_string()), mtime)
}

async fn list_tasks(State(store): State<Store>) -> Result<Json<Vec<Task>>, ApiError> {
    Ok(Json(store.list_tasks().await?))
}

async fn create_task(
    State(app): State<AppState>,
    Json(new): Json<NewTask>,
) -> Result<(StatusCode, Json<Task>), ApiError> {
    let task = Task::new(TaskKey::new(new.key), new.title, new.workflow);
    app.store.create_task(&task).await?;
    app.emit(crate::events::Topic::Tasks);
    Ok((StatusCode::CREATED, Json(task)))
}

async fn get_task(
    State(store): State<Store>,
    Path(key): Path<String>,
) -> Result<Json<Task>, ApiError> {
    let key = TaskKey::new(key);
    let task = store
        .get_task(&key)
        .await?
        .ok_or(StoreError::NotFound(key))?;
    Ok(Json(task))
}

async fn list_events(
    State(store): State<Store>,
    Path(key): Path<String>,
) -> Result<Json<Vec<EventRecord>>, ApiError> {
    Ok(Json(store.events(&TaskKey::new(key)).await?))
}

async fn post_event(
    State(store): State<Store>,
    Path(key): Path<String>,
    Json(event): Json<Event>,
) -> Result<Json<Task>, ApiError> {
    Ok(Json(
        store
            .apply_event(&TaskKey::new(key), event, Utc::now())
            .await?,
    ))
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ErrorBody {
    pub error: String,
}

pub struct ApiError {
    status: StatusCode,
    message: String,
}

impl ApiError {
    pub fn bad_request(message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, message)
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self::new(StatusCode::NOT_FOUND, message)
    }

    pub fn unprocessable(message: impl Into<String>) -> Self {
        Self::new(StatusCode::UNPROCESSABLE_ENTITY, message)
    }

    pub fn conflict(message: impl Into<String>) -> Self {
        Self::new(StatusCode::CONFLICT, message)
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self::new(StatusCode::INTERNAL_SERVER_ERROR, message)
    }

    fn new(status: StatusCode, message: impl Into<String>) -> Self {
        Self {
            status,
            message: message.into(),
        }
    }
}

impl From<StoreError> for ApiError {
    fn from(e: StoreError) -> Self {
        let status = match &e {
            StoreError::NotFound(_) | StoreError::ProjectNotFound(_) => StatusCode::NOT_FOUND,
            StoreError::AlreadyExists(_)
            | StoreError::ProjectExists(_)
            | StoreError::Transition(_) => StatusCode::CONFLICT,
            StoreError::Db(_)
            | StoreError::Migrate(_)
            | StoreError::Json(_)
            | StoreError::Corrupt(_) => StatusCode::INTERNAL_SERVER_ERROR,
        };
        Self::new(status, e.to_string())
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        if self.status.is_server_error() {
            tracing::error!(error = %self.message, "request failed");
        }
        (
            self.status,
            Json(ErrorBody {
                error: self.message,
            }),
        )
            .into_response()
    }
}

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use axum::http::Request;
    use http_body_util::BodyExt;
    use serde_json::{Value, json};
    use tower::ServiceExt;

    use super::*;

    async fn call(
        app: &Router,
        method: &str,
        uri: &str,
        body: Option<Value>,
    ) -> (StatusCode, Value) {
        let mut req = Request::builder().method(method).uri(uri);
        let body = match body {
            Some(json) => {
                req = req.header("content-type", "application/json");
                Body::from(json.to_string())
            }
            None => Body::empty(),
        };
        let res = app.clone().oneshot(req.body(body).unwrap()).await.unwrap();
        let status = res.status();
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        let value = if bytes.is_empty() {
            Value::Null
        } else {
            serde_json::from_slice(&bytes).unwrap()
        };
        (status, value)
    }

    async fn app() -> Router {
        let config = crate::config::Config {
            home: std::env::temp_dir().join(format!("delilah-api-{}", uuid::Uuid::new_v4())),
            addr: crate::config::DEFAULT_ADDR.parse().unwrap(),
            allow_file_protocol: false,
            notify: false,
        };
        router(AppState::new(Store::in_memory().await.unwrap(), config))
    }

    #[tokio::test]
    async fn creates_and_lists_tasks() {
        let app = app().await;
        let (status, task) = call(
            &app,
            "POST",
            "/api/v1/tasks",
            Some(json!({"key": "PROJ-1", "title": "Card retries"})),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED);
        assert_eq!(task["status"], json!({"type": "queued"}));
        assert_eq!(task["workflow"], "feature");

        let (status, _) = call(
            &app,
            "POST",
            "/api/v1/tasks",
            Some(json!({"key": "PROJ-1", "title": "Again"})),
        )
        .await;
        assert_eq!(status, StatusCode::CONFLICT);

        let (status, list) = call(&app, "GET", "/api/v1/tasks", None).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(list.as_array().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn applies_events_and_reports_invalid_ones() {
        let app = app().await;
        call(
            &app,
            "POST",
            "/api/v1/tasks",
            Some(json!({"key": "PROJ-1", "title": "Card retries"})),
        )
        .await;

        let (status, task) = call(
            &app,
            "POST",
            "/api/v1/tasks/PROJ-1/events",
            Some(json!({"type": "start"})),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(task["status"]["type"], "preparing-workspace");

        let (status, body) = call(
            &app,
            "POST",
            "/api/v1/tasks/PROJ-1/events",
            Some(json!({"type": "agent-finished"})),
        )
        .await;
        assert_eq!(status, StatusCode::CONFLICT);
        assert!(body["error"].as_str().unwrap().contains("agent-finished"));

        let (status, history) = call(&app, "GET", "/api/v1/tasks/PROJ-1/events", None).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(history[0]["event"], json!({"type": "start"}));
        assert_eq!(history.as_array().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn unknown_tasks_are_not_found() {
        let app = app().await;
        let (status, _) = call(&app, "GET", "/api/v1/tasks/PROJ-404", None).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        let (status, _) = call(&app, "GET", "/api/v1/tasks/PROJ-404/events", None).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }
}

#[cfg(test)]
mod cors_tests {
    use axum::body::Body;
    use axum::http::Request;
    use tower::ServiceExt;

    use super::*;

    async fn preflight(origin: &str) -> Option<String> {
        let config = crate::config::Config {
            home: std::env::temp_dir(),
            addr: crate::config::DEFAULT_ADDR.parse().unwrap(),
            allow_file_protocol: false,
            notify: false,
        };
        let app = router(AppState::new(Store::in_memory().await.unwrap(), config));
        let req = Request::builder()
            .method("OPTIONS")
            .uri("/api/v1/runs")
            .header("origin", origin)
            .header("access-control-request-method", "POST")
            .header("access-control-request-headers", "content-type")
            .body(Body::empty())
            .unwrap();
        let res = app.oneshot(req).await.unwrap();
        res.headers()
            .get("access-control-allow-origin")
            .map(|v| v.to_str().unwrap().to_string())
    }

    #[tokio::test]
    async fn only_the_desktop_app_may_call_from_a_browser() {
        assert_eq!(
            preflight("tauri://localhost").await.as_deref(),
            Some("tauri://localhost")
        );
        assert_eq!(
            preflight("http://localhost:1420").await.as_deref(),
            Some("http://localhost:1420")
        );
        assert_eq!(preflight("https://evil.example").await, None);
    }
}
