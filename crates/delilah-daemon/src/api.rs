//! HTTP API of the daemon. Every client (CLI, desktop app, mobile web) goes through
//! it: nothing else reads the database.

use axum::extract::{Path, Request, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::middleware::{self, Next};
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
    let preview = state.config.ui_origin.clone();
    let origins = APP_ORIGINS
        .iter()
        .copied()
        .chain(preview.as_deref())
        .filter_map(|o| HeaderValue::from_str(o).ok());
    let cors = CorsLayer::new()
        .allow_origin(AllowOrigin::list(origins))
        .allow_methods(Any)
        .allow_headers(Any);
    Router::new()
        .route("/api/v1/health", get(health))
        .route("/api/v1/tasks", get(list_tasks).post(create_task))
        .route(
            "/api/v1/tasks/{key}",
            get(get_task)
                .patch(update_task)
                .delete(crate::actions::delete),
        )
        .route(
            "/api/v1/tasks/{key}/events",
            get(list_events).post(post_event),
        )
        .route("/api/v1/tasks/{key}/terminal", get(terminal::task_terminal))
        .route(
            "/api/v1/tasks/{key}/sessions",
            get(session_api::list_for_task).post(session_api::open),
        )
        .route("/api/v1/prs", get(crate::pr_api::summaries))
        .route(
            "/api/v1/tasks/{key}/prs",
            get(crate::pr_api::task_prs).post(crate::pr_api::open),
        )
        .route(
            "/api/v1/tasks/{key}/prs/proposal",
            get(crate::pr_api::proposal),
        )
        .route(
            "/api/v1/tasks/{key}/prs/refresh",
            post(crate::pr_api::refresh),
        )
        .route("/api/v1/tasks/{key}/prs/link", post(crate::pr_api::link))
        .route(
            "/api/v1/tasks/{key}/prs/{id}",
            axum::routing::delete(crate::pr_api::unlink),
        )
        .route(
            "/api/v1/pr-notices/{id}/forward",
            post(crate::pr_api::forward),
        )
        .route(
            "/api/v1/pr-notices/{id}/dismiss",
            post(crate::pr_api::dismiss),
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
        .route("/api/v1/tasks/{key}/phase", post(crate::phases::set))
        .route(
            "/api/v1/tasks/{key}/saving-mode",
            post(crate::handoff::set_saving_mode),
        )
        .route("/api/v1/tasks/{key}/handoff", post(crate::handoff::act))
        .route(
            "/api/v1/tasks/{key}/review-hint/dismiss",
            post(crate::phases::dismiss),
        )
        .route("/api/v1/sessions/{id}/reply", post(inbox::reply))
        .route(
            "/api/v1/sessions/{id}/conversation",
            get(crate::conversation::get),
        )
        .route(
            "/api/v1/sessions/{id}/messages",
            post(crate::conversation::send),
        )
        .route(
            "/api/v1/sessions/{id}/messages/{entry}",
            axum::routing::delete(crate::conversation::remove),
        )
        .route(
            "/api/v1/sessions/{id}/stop",
            post(crate::conversation::stop),
        )
        .route("/api/v1/tasks/{key}/changes", get(changes::list))
        .route("/api/v1/tasks/{key}/changes/diff", get(changes::diff))
        .route("/api/v1/tasks/{key}/tree", get(files::tree))
        .route("/api/v1/tasks/{key}/files", get(files::all))
        .route("/api/v1/tasks/{key}/file", get(files::get).put(files::put))
        .route("/api/v1/tasks/{key}/image", get(files::image))
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
        .layer(middleware::from_fn(move |req, next| {
            guard(preview.clone(), req, next)
        }))
}

/// The API has no authentication, so only the app and local tools may reach it:
/// - a browser `Origin` other than the app's is refused. CORS alone is not enough:
///   WebSockets skip it, and a form POST with no body runs before CORS hides the reply.
///   The CLI, curl and the agents' hooks send no `Origin`.
/// - the `Host` must be a loopback name, so a page that rebinds its own domain to
///   127.0.0.1 is refused too (DNS rebinding).
///
/// `preview` is the one more origin a preview daemon lets in (`DELILAH_UI_ORIGIN`).
async fn guard(preview: Option<String>, req: Request, next: Next) -> Response {
    match refusal(req.headers(), preview.as_deref()) {
        Some(reason) => {
            tracing::warn!(uri = %req.uri(), "refused: {reason}");
            ApiError::new(StatusCode::FORBIDDEN, reason).into_response()
        }
        None => next.run(req).await,
    }
}

fn refusal(headers: &HeaderMap, preview: Option<&str>) -> Option<&'static str> {
    if let Some(origin) = headers.get(header::ORIGIN)
        && !APP_ORIGINS
            .iter()
            .copied()
            .chain(preview)
            .any(|o| origin == o)
    {
        return Some("requests from other origins are not allowed");
    }
    if let Some(host) = headers.get(header::HOST)
        && !host.to_str().is_ok_and(is_loopback_host)
    {
        return Some("the Host must be a loopback address");
    }
    None
}

/// `127.0.0.1:7420`, `localhost`, `[::1]:7420`… The port does not matter: a rebinding
/// page can only control the name.
fn is_loopback_host(host: &str) -> bool {
    let name = match host.strip_prefix('[') {
        Some(rest) => rest.split(']').next().unwrap_or(""),
        None => host.rsplit_once(':').map_or(host, |(name, _)| name),
    };
    name.eq_ignore_ascii_case("localhost")
        || name
            .parse::<std::net::IpAddr>()
            .is_ok_and(|ip| ip.is_loopback())
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

/// A task saved without starting it: the backlog.
#[derive(Debug, Serialize, Deserialize)]
pub struct NewTask {
    /// May be empty for a `free` task: Delilah names it `LIBRE-<n>`.
    #[serde(default)]
    pub key: String,
    pub title: String,
    /// The project it will run against. The app and the CLI always send one; bare
    /// tasks (no project) are only for driving the state machine by hand.
    pub project: Option<String>,
    #[serde(default)]
    pub notes: Option<String>,
    /// A task without a Jira ticket.
    #[serde(default)]
    pub free: bool,
    /// Where it will work once started: the project's checkout or a worktree.
    #[serde(default)]
    pub in_checkout: Option<bool>,
    /// Each phase in a fresh session; absent, the `saving-mode` preference.
    #[serde(default)]
    pub saving_mode: Option<bool>,
    /// Defaults to the project's workflow (`feature` without a project).
    pub workflow: Option<String>,
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
    let task = build_task(&app, new).await?;
    app.store.create_task(&task).await?;
    app.emit(crate::events::Topic::Tasks);
    Ok((StatusCode::CREATED, Json(task)))
}

/// Everything `POST /tasks` does short of storing: validates, resolves the project and
/// key, and builds the backlog task.
pub(crate) async fn build_task(app: &AppState, new: NewTask) -> Result<Task, ApiError> {
    if new.title.trim().is_empty() {
        return Err(ApiError::bad_request("say what the task is about"));
    }
    let project = match &new.project {
        Some(name) => Some(app.store.project(name).await?),
        None => None,
    };
    let key = match new.free && new.key.trim().is_empty() {
        true => crate::run::next_free_key(&app).await?,
        false => new.key,
    };
    let key = TaskKey::parse(key).map_err(|e| ApiError::bad_request(e.to_string()))?;
    let workflow = new
        .workflow
        .or_else(|| project.as_ref().map(|p| p.workflow.clone()))
        .unwrap_or_else(default_workflow);
    let mut task = Task::new(key, new.title.trim(), workflow);
    task.project = project.map(|p| p.name);
    task.notes = new.notes.filter(|n| !n.trim().is_empty());
    task.in_checkout = new.in_checkout;
    task.saving_mode = match new.saving_mode {
        Some(on) => on,
        None => crate::handoff::saving_mode_preference(app).await,
    };
    Ok(task)
}

/// Edits a backlog task (title, notes, project, workflow, where); 409 once it has started.
async fn update_task(
    State(app): State<AppState>,
    Path(key): Path<String>,
    Json(body): Json<UpdateTask>,
) -> Result<Json<Task>, ApiError> {
    let UpdateTask {
        mut change,
        saving_mode,
    } = body;
    if let Some(title) = &mut change.title {
        *title = title.trim().to_string();
        if title.is_empty() {
            return Err(ApiError::bad_request("a task needs a title"));
        }
    }
    let key = TaskKey::new(key);
    let mut task = app.store.update_backlog_task(&key, &change).await?;
    if let Some(on) = saving_mode.filter(|on| *on != task.saving_mode) {
        task = crate::handoff::apply_saving_mode(&app, &key, on).await?;
    }
    app.emit(crate::events::Topic::Tasks);
    Ok(Json(task))
}

/// `PATCH /api/v1/tasks/{key}`: the backlog change plus saving mode.
#[derive(Debug, Deserialize)]
struct UpdateTask {
    #[serde(flatten)]
    change: crate::store::BacklogChange,
    #[serde(default)]
    saving_mode: Option<bool>,
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
    /// Refused because work in a worktree would be lost: the client may offer to force it.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub unpushed: bool,
    /// A task paused by the limit: when it resumes by itself.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resume_at: Option<chrono::DateTime<Utc>>,
    /// Why a 409 happened, for the client to tell apart (`no-commits`, `has-pr`...).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

pub struct ApiError {
    pub(crate) status: StatusCode,
    pub(crate) message: String,
    pub(crate) unpushed: bool,
    pub(crate) resume_at: Option<chrono::DateTime<Utc>>,
    pub(crate) reason: Option<String>,
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

    /// A 409 the client tells apart by `reason`.
    pub fn conflict_reason(message: impl Into<String>, reason: &str) -> Self {
        Self {
            reason: Some(reason.to_string()),
            ..Self::conflict(message)
        }
    }

    /// A 409 for work that would be lost; `?force=true` goes ahead anyway.
    pub fn unpushed(message: impl Into<String>) -> Self {
        Self {
            unpushed: true,
            ..Self::conflict(message)
        }
    }

    /// A 409 for a task paused by the limit; the body says when it resumes.
    pub fn paused(message: impl Into<String>, resume_at: chrono::DateTime<Utc>) -> Self {
        Self {
            resume_at: Some(resume_at),
            ..Self::conflict(message)
        }
    }

    pub fn too_large(message: impl Into<String>) -> Self {
        Self::new(StatusCode::PAYLOAD_TOO_LARGE, message)
    }

    pub fn unsupported_type(message: impl Into<String>) -> Self {
        Self::new(StatusCode::UNSUPPORTED_MEDIA_TYPE, message)
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
            unpushed: false,
            resume_at: None,
            reason: None,
        }
    }
}

impl From<StoreError> for ApiError {
    fn from(e: StoreError) -> Self {
        let status = match &e {
            StoreError::NotFound(_) | StoreError::ProjectNotFound(_) => StatusCode::NOT_FOUND,
            StoreError::AlreadyExists(_)
            | StoreError::NotInBacklog(_)
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
                unpushed: self.unpushed,
                resume_at: self.resume_at,
                reason: self.reason,
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
            ui_origin: None,
            agent_program: None,
            ..crate::config::Config::test_defaults()
        };
        router(AppState::new(Store::in_memory().await.unwrap(), config))
    }

    /// The API with a `web` repo project registered (its folder is never touched).
    async fn app_with_project() -> (Router, AppState) {
        let config = crate::config::Config {
            home: std::env::temp_dir().join(format!("delilah-api-{}", uuid::Uuid::new_v4())),
            addr: crate::config::DEFAULT_ADDR.parse().unwrap(),
            allow_file_protocol: false,
            notify: false,
            ui_origin: None,
            agent_program: None,
            ..crate::config::Config::test_defaults()
        };
        let state = AppState::new(Store::in_memory().await.unwrap(), config);
        state
            .store
            .create_project(&delilah_core::Project {
                name: "web".into(),
                path: "/w/web".into(),
                kind: delilah_core::ProjectKind::Repo,
                workflow: "custom".into(),
            })
            .await
            .unwrap();
        (router(state.clone()), state)
    }

    #[tokio::test]
    async fn saves_backlog_tasks_without_starting_them() {
        let (app, state) = app_with_project().await;
        let (status, task) = call(
            &app,
            "POST",
            "/api/v1/tasks",
            Some(
                json!({"free": true, "title": " Reintentos ", "project": "web", "notes": "con backoff", "in_checkout": false}),
            ),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED, "{task}");
        assert_eq!(task["key"], "LIBRE-1");
        assert_eq!(task["title"], "Reintentos");
        assert_eq!(task["status"], json!({"type": "queued"}));
        assert_eq!(task["project"], "web");
        assert_eq!(task["notes"], "con backoff");
        assert_eq!(task["workflow"], "custom", "the project's workflow");
        assert_eq!(task["in_checkout"], false, "it will work in a worktree");
        let key = TaskKey::new("LIBRE-1");
        assert!(state.store.workspace(&key).await.unwrap().is_none());
        assert!(
            state
                .store
                .primary_session("LIBRE-1")
                .await
                .unwrap()
                .is_none()
        );

        let (status, _) = call(
            &app,
            "POST",
            "/api/v1/tasks",
            Some(json!({"key": "PROJ-1", "title": "x", "project": "nope"})),
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND);

        let (status, task) = call(
            &app,
            "PATCH",
            "/api/v1/tasks/LIBRE-1",
            Some(json!({"notes": "sin backoff", "in_checkout": true})),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{task}");
        assert_eq!(task["notes"], "sin backoff");
        assert_eq!(task["in_checkout"], true, "now in the checkout");
        assert_eq!(task["title"], "Reintentos");

        call(
            &app,
            "POST",
            "/api/v1/tasks/LIBRE-1/events",
            Some(json!({"type": "start"})),
        )
        .await;
        let (status, _) = call(
            &app,
            "PATCH",
            "/api/v1/tasks/LIBRE-1",
            Some(json!({"title": "y"})),
        )
        .await;
        assert_eq!(
            status,
            StatusCode::CONFLICT,
            "only backlog tasks can be edited"
        );
    }

    #[tokio::test]
    async fn refuses_backlog_tasks_it_cannot_name_or_place() {
        let (app, _) = app_with_project().await;
        for body in [
            json!({"title": "x", "project": "web"}),
            json!({"key": "PROJ-1", "title": "  ", "project": "web"}),
        ] {
            let (status, _) = call(&app, "POST", "/api/v1/tasks", Some(body.clone())).await;
            assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
        }
        call(
            &app,
            "POST",
            "/api/v1/tasks",
            Some(json!({"key": "PROJ-1", "title": "x", "project": "web"})),
        )
        .await;
        let (status, _) = call(
            &app,
            "PATCH",
            "/api/v1/tasks/PROJ-1",
            Some(json!({"project": "nope"})),
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        let (status, _) = call(
            &app,
            "PATCH",
            "/api/v1/tasks/PROJ-1",
            Some(json!({"title": " "})),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn free_task_numbers_are_never_handed_out_twice() {
        let (app, state) = app_with_project().await;
        for _ in 0..2 {
            call(
                &app,
                "POST",
                "/api/v1/tasks",
                Some(json!({"free": true, "title": "idea", "project": "web"})),
            )
            .await;
        }
        state
            .store
            .delete_task(&TaskKey::new("LIBRE-2"))
            .await
            .unwrap();
        let (_, task) = call(
            &app,
            "POST",
            "/api/v1/tasks",
            Some(json!({"free": true, "title": "otra", "project": "web"})),
        )
        .await;
        assert_eq!(task["key"], "LIBRE-3");
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

    #[tokio::test]
    async fn saving_mode_on_a_task_and_its_handoff_actions() {
        let (app, state) = app_with_project().await;
        let key = TaskKey::new("PROJ-1");
        let mut t = Task::new(key.clone(), "t", "feature");
        t.saving_mode = true;
        state.store.create_task(&t).await.unwrap();
        let offered = delilah_core::Handoff {
            to: delilah_core::Step::Implement,
            stage: delilah_core::HandoffStage::Offered,
            error: None,
        };
        state.store.set_handoff(&key, Some(&offered)).await.unwrap();
        let uri = "/api/v1/tasks/PROJ-1/handoff";

        let (status, _) = call(&app, "POST", uri, Some(json!({"action": "retry"}))).await;
        assert_eq!(status, StatusCode::CONFLICT, "retry needs a failed handoff");
        let (status, _) = call(&app, "POST", uri, Some(json!({"action": "dance"}))).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        let (status, _) = call(
            &app,
            "POST",
            "/api/v1/tasks/NOPE-1/handoff",
            Some(json!({"action": "decline"})),
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND);

        let (status, task) = call(&app, "POST", uri, Some(json!({"action": "decline"}))).await;
        assert_eq!(status, StatusCode::OK, "{task}");
        assert_eq!(task["saving_mode"], false);
        assert!(task["handoff"].is_null());
        let (status, _) = call(&app, "POST", uri, Some(json!({"action": "decline"}))).await;
        assert_eq!(status, StatusCode::CONFLICT, "nothing left to decline");

        // The saving-mode route: on keeps a handoff, off clears it.
        let uri = "/api/v1/tasks/PROJ-1/saving-mode";
        let (status, task) = call(&app, "POST", uri, Some(json!({"on": true}))).await;
        assert_eq!(
            (status, &task["saving_mode"]),
            (StatusCode::OK, &json!(true))
        );
        state.store.set_handoff(&key, Some(&offered)).await.unwrap();
        let (_, task) = call(&app, "POST", uri, Some(json!({"on": false}))).await;
        assert_eq!(task["saving_mode"], false);
        assert!(task["handoff"].is_null());
        let (status, _) = call(
            &app,
            "POST",
            "/api/v1/tasks/NOPE-1/saving-mode",
            Some(json!({"on": true})),
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn a_backlog_task_takes_saving_mode_from_the_request_or_the_preference() {
        let (app, state) = app_with_project().await;
        let new = |extra: Value| {
            let mut body = json!({"free": true, "title": "T", "project": "web"});
            body.as_object_mut()
                .unwrap()
                .extend(extra.as_object().unwrap().clone());
            body
        };
        let (_, task) = call(&app, "POST", "/api/v1/tasks", Some(new(json!({})))).await;
        assert_eq!(task["saving_mode"], false);
        let (_, task) = call(
            &app,
            "POST",
            "/api/v1/tasks",
            Some(new(json!({"saving_mode": true}))),
        )
        .await;
        assert_eq!(task["saving_mode"], true);
        let key = task["key"].as_str().unwrap().to_string();

        let uri = format!("/api/v1/tasks/{key}");
        let (_, task) = call(&app, "PATCH", &uri, Some(json!({"title": "U"}))).await;
        assert_eq!(task["saving_mode"], true, "left out, it stays");
        let (_, task) = call(&app, "PATCH", &uri, Some(json!({"saving_mode": false}))).await;
        assert_eq!(task["saving_mode"], false);
        let stored = state
            .store
            .get_task(&TaskKey::new(&key))
            .await
            .unwrap()
            .unwrap();
        assert!(!stored.saving_mode);

        state
            .store
            .set_setting("pref:saving-mode", &json!(true))
            .await
            .unwrap();
        let (_, task) = call(&app, "POST", "/api/v1/tasks", Some(new(json!({})))).await;
        assert_eq!(task["saving_mode"], true, "the preference is the default");
    }

    #[tokio::test]
    async fn patching_saving_mode_off_clears_a_pending_handoff() {
        let (app, state) = app_with_project().await;
        let body = json!({"free": true, "title": "T", "project": "web", "saving_mode": true});
        let (_, task) = call(&app, "POST", "/api/v1/tasks", Some(body)).await;
        let key = TaskKey::new(task["key"].as_str().unwrap());
        let offered = delilah_core::Handoff {
            to: delilah_core::Step::Implement,
            stage: delilah_core::HandoffStage::Offered,
            error: None,
        };
        state.store.set_handoff(&key, Some(&offered)).await.unwrap();
        let uri = format!("/api/v1/tasks/{key}");
        let (_, task) = call(&app, "PATCH", &uri, Some(json!({"saving_mode": false}))).await;
        assert_eq!(task["saving_mode"], false);
        assert!(task["handoff"].is_null());
        let stored = state.store.get_task(&key).await.unwrap().unwrap();
        assert!(stored.handoff.is_none());
    }
}

#[cfg(test)]
mod cors_tests {
    use axum::body::Body;
    use axum::http::Request;
    use tower::ServiceExt;

    use super::*;

    async fn preflight(origin: &str) -> Option<String> {
        preflight_with(origin, None).await
    }

    async fn preflight_with(origin: &str, ui_origin: Option<&str>) -> Option<String> {
        let config = crate::config::Config {
            home: std::env::temp_dir(),
            addr: crate::config::DEFAULT_ADDR.parse().unwrap(),
            allow_file_protocol: false,
            notify: false,
            ui_origin: ui_origin.map(str::to_string),
            agent_program: None,
            ..crate::config::Config::test_defaults()
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
        assert_eq!(preflight("http://localhost:1520").await, None);
    }

    #[tokio::test]
    async fn a_preview_daemon_also_lets_its_own_ui_in() {
        let ui = "http://localhost:1520";
        assert_eq!(preflight_with(ui, Some(ui)).await.as_deref(), Some(ui));
        assert_eq!(
            preflight_with("http://localhost:1521", Some(ui)).await,
            None
        );
    }

    async fn status(method: &str, uri: &str, headers: &[(&str, &str)]) -> StatusCode {
        let config = crate::config::Config {
            home: std::env::temp_dir(),
            addr: crate::config::DEFAULT_ADDR.parse().unwrap(),
            allow_file_protocol: false,
            notify: false,
            ui_origin: None,
            agent_program: None,
            ..crate::config::Config::test_defaults()
        };
        let app = router(AppState::new(Store::in_memory().await.unwrap(), config));
        let mut req = Request::builder().method(method).uri(uri);
        for (name, value) in headers {
            req = req.header(*name, *value);
        }
        app.oneshot(req.body(Body::empty()).unwrap())
            .await
            .unwrap()
            .status()
    }

    #[tokio::test]
    async fn other_origins_cannot_post_or_open_websockets() {
        // A form POST with no body needs no preflight: it must still be refused.
        let evil = [
            ("origin", "https://evil.example"),
            ("host", "127.0.0.1:7420"),
        ];
        assert_eq!(
            status("POST", "/api/v1/tasks/PROJ-1/done", &evil).await,
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            status("GET", "/api/v1/tasks/PROJ-1/terminal", &evil).await,
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            status("GET", "/api/v1/tasks", &[("origin", "null")]).await,
            StatusCode::FORBIDDEN
        );
        // The app, and tools that send no Origin, get through.
        let app = [("origin", "tauri://localhost"), ("host", "127.0.0.1:7420")];
        assert_eq!(status("GET", "/api/v1/tasks", &app).await, StatusCode::OK);
        assert_eq!(
            status("GET", "/api/v1/tasks", &[("host", "localhost:7420")]).await,
            StatusCode::OK
        );
    }

    #[tokio::test]
    async fn rebound_host_names_are_refused() {
        for host in [
            "evil.example:7420",
            "evil.example",
            "127.0.0.1.evil.example:7420",
        ] {
            assert_eq!(
                status("GET", "/api/v1/tasks", &[("host", host)]).await,
                StatusCode::FORBIDDEN,
                "{host}"
            );
        }
        for host in ["127.0.0.1:7420", "localhost", "[::1]:7420", "127.0.0.1"] {
            assert_eq!(
                status("GET", "/api/v1/tasks", &[("host", host)]).await,
                StatusCode::OK,
                "{host}"
            );
        }
    }
}
