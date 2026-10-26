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
    actions, asks, changes, events, files, hooks, inbox, jira, live, permissions, preferences,
    projects, run, session_api, specs, terminal,
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
            "/api/v1/tasks/{key}/runtime",
            post(crate::runtime_api::set_runtime),
        )
        .route(
            "/api/v1/tasks/{key}/sessions",
            get(session_api::list_for_task).post(session_api::open),
        )
        .route(
            "/api/v1/tasks/{key}/preview",
            get(crate::preview_api::get_preview)
                .post(crate::preview_api::start_preview)
                .delete(crate::preview_api::stop_preview),
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
        // PUT answers 400 for a bad name, a value over 64 KiB, or (for `agent-permissions`)
        // anything but the JSON strings "auto" or "default"; the stored value is kept.
        .route(
            "/api/v1/preferences/{key}",
            get(preferences::get).put(preferences::put),
        )
        .route(
            "/api/v1/sessions/{id}/terminal",
            get(terminal::session_terminal),
        )
        .route("/api/v1/sessions/{id}/live", get(live::session_live))
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
        .route("/api/v1/tasks/{key}/reopen", post(actions::reopen))
        .route(
            "/api/v1/tasks/{key}/prs/{id}/merge",
            post(crate::pr_api::merge::merge),
        )
        .route("/api/v1/tasks/{key}/setup", get(crate::setup::info))
        .route("/api/v1/tasks/{key}/setup/log", get(crate::setup::log))
        .route(
            "/api/v1/tasks/{key}/base-status",
            get(crate::base_update::status),
        )
        .route(
            "/api/v1/tasks/{key}/update-branch",
            post(crate::base_update::update),
        )
        .route("/api/v1/inbox/{id}/dismiss", post(inbox::dismiss))
        .route("/api/v1/inbox/clear", post(inbox::clear))
        .route("/api/v1/inbox/restore", post(inbox::restore))
        .route("/api/v1/jira/issues/{key}/subtasks", get(jira::subtasks))
        .route(
            "/api/v1/sessions/{id}/runtime",
            post(crate::runtime_api::set_session_runtime),
        )
        .route("/api/v1/tasks/{key}/resume", post(actions::resume))
        .route("/api/v1/tasks/{key}/done", post(actions::done))
        .route("/api/v1/tasks/{key}/archive", post(actions::archive))
        .route("/api/v1/tasks/{key}/setup/retry", post(setup_retry))
        .route("/api/v1/tasks/{key}/setup/skip", post(setup_skip))
        .route("/api/v1/tasks/{key}/setup/cancel", post(setup_cancel))
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
        .route("/api/v1/tasks/{key}/autopilot", post(crate::autopilot::act))
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
            "/api/v1/sessions/{id}/conversation/tools/{tool_use_id}",
            get(crate::conversation::tool),
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
            "/api/v1/projects/{name}/settings",
            axum::routing::put(projects::update_settings),
        )
        .route(
            "/api/v1/projects/{name}/submodules",
            get(projects::submodules),
        )
        .route(
            "/api/v1/projects/{name}/branch-name",
            post(crate::branch_name::propose_branch),
        )
        .route(
            "/api/v1/projects/{name}/suggest-submodules",
            post(projects::suggest_submodules),
        )
        .route("/api/v1/runs", post(run::start_run))
        .route("/hooks/claude", post(hooks::claude_hook))
        .route("/hooks/claude/statusline", post(hooks::claude_statusline))
        .route("/hooks/claude/ask", post(asks::hook))
        .route("/hooks/claude/guard", post(hooks::guard_hook))
        .route("/api/v1/tasks/{key}/branch", post(crate::branching::retry))
        .route("/api/v1/asks/{id}/answer", post(asks::answer))
        .route("/api/v1/asks/{id}/release", post(asks::release))
        .route(
            "/api/v1/sessions/{id}/permissions/{ask}",
            post(permissions::answer),
        )
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

/// The body of the action endpoints (`handoff`, `autopilot`): `{ "action": "…" }`.
#[derive(Debug, Deserialize)]
pub(crate) struct ActionReq {
    pub action: String,
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
    /// The absolute folder Delilah keeps its state in (`~/.delilah`).
    #[serde(default)]
    pub home: String,
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
    /// Autopilot on: forces saving mode on, and conflicts with it being off.
    #[serde(default)]
    pub autopilot: Option<bool>,
    /// Defaults to the project's workflow (`feature` without a project).
    pub workflow: Option<String>,
    /// `chat` or `terminal`; absent, chat.
    #[serde(default)]
    pub view: Option<String>,
}

/// Parses a `view` from a request; anything but `chat` or `terminal` is a 400.
fn parse_view(view: &str) -> Result<delilah_core::TaskView, ApiError> {
    delilah_core::TaskView::parse(view).ok_or_else(|| {
        ApiError::bad_request(format!(
            "view must be \"chat\" or \"terminal\", not {view:?}"
        ))
    })
}

fn default_workflow() -> String {
    "feature".into()
}

/// `POST /api/v1/tasks/{key}/setup/retry`: from `setup-failed`, runs the setup again.
async fn setup_retry(
    State(app): State<AppState>,
    Path(key): Path<String>,
) -> Result<(StatusCode, Json<Task>), ApiError> {
    let key = TaskKey::new(key);
    crate::setup::retry(&app, &key).await?;
    accepted(&app, &key).await
}

/// `POST /api/v1/tasks/{key}/setup/skip`: from `setup-failed`, launches the agent.
async fn setup_skip(
    State(app): State<AppState>,
    Path(key): Path<String>,
) -> Result<(StatusCode, Json<Task>), ApiError> {
    let key = TaskKey::new(key);
    crate::setup::skip(&app, &key).await?;
    accepted(&app, &key).await
}

/// `POST /api/v1/tasks/{key}/setup/cancel`: kills the running setup; the task ends
/// in `setup-failed` ("cancelled") a moment later.
async fn setup_cancel(
    State(app): State<AppState>,
    Path(key): Path<String>,
) -> Result<(StatusCode, Json<Task>), ApiError> {
    let key = TaskKey::new(key);
    crate::setup::cancel(&app, &key).await?;
    accepted(&app, &key).await
}

async fn accepted(app: &AppState, key: &TaskKey) -> Result<(StatusCode, Json<Task>), ApiError> {
    let task = app
        .store
        .get_task(key)
        .await?
        .ok_or_else(|| ApiError::not_found(format!("no task {key}")))?;
    Ok((StatusCode::ACCEPTED, Json(task)))
}

async fn health(State(app): State<AppState>) -> Json<Health> {
    let (exe, exe_mtime) = EXE.get_or_init(running_binary).clone();
    Json(Health {
        version: env!("CARGO_PKG_VERSION").into(),
        exe,
        exe_mtime,
        home: app.config.home.display().to_string(),
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

/// `POST /tasks`: a [`NewTask`] and, on create only, whether it runs in chat mode.
#[derive(Debug, Deserialize)]
struct CreateTask {
    #[serde(flatten)]
    new: NewTask,
    #[serde(default)]
    chat_mode: bool,
}

async fn create_task(
    State(app): State<AppState>,
    Json(body): Json<CreateTask>,
) -> Result<(StatusCode, Json<Task>), ApiError> {
    let mut task = build_task(&app, body.new).await?;
    task.chat_mode = body.chat_mode;
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
    check_autopilot(new.autopilot, new.saving_mode)?;
    let view = new.view.as_deref().map(parse_view).transpose()?;
    let project = match &new.project {
        Some(name) => Some(app.store.project(name).await?),
        None => None,
    };
    let key = match new.free && new.key.trim().is_empty() {
        true => crate::run::next_free_key(app).await?,
        false => new.key,
    };
    let key = TaskKey::parse(key).map_err(|e| ApiError::bad_request(e.to_string()))?;
    let workflow = new
        .workflow
        .or_else(|| project.as_ref().map(|p| p.workflow.clone()))
        .unwrap_or_else(default_workflow);
    if new.autopilot == Some(true) && !crate::autopilot::allowed(&key, &workflow) {
        return Err(crate::autopilot::not_spec_workflow(&key));
    }
    let mut task = Task::new(key, new.title.trim(), workflow);
    if crate::branching::is_free(&task.key) {
        task.title_source = delilah_core::TitleSource::Initial;
    }
    task.project = project.map(|p| p.name);
    task.notes = new.notes.filter(|n| !n.trim().is_empty());
    task.in_checkout = new.in_checkout;
    task.view = view.unwrap_or_default();
    task.saving_mode = match new.saving_mode {
        Some(on) => on,
        None => crate::handoff::saving_mode_preference(app).await,
    };
    if new.autopilot == Some(true) {
        task.saving_mode = true;
        task.autopilot = Some(crate::autopilot::fresh());
    }
    Ok(task)
}

/// `autopilot: true` with `saving_mode: false` is a contradiction.
fn check_autopilot(autopilot: Option<bool>, saving_mode: Option<bool>) -> Result<(), ApiError> {
    match (autopilot, saving_mode) {
        (Some(true), Some(false)) => Err(ApiError::bad_request(
            "autopilot needs saving mode: it cannot run with saving mode off",
        )),
        _ => Ok(()),
    }
}

/// Edits a backlog task (title, notes, project, workflow, where); 409 once it has started.
/// The title alone can change on any task but an archived one; the view, saving mode and
/// autopilot change in any state.
async fn update_task(
    State(app): State<AppState>,
    Path(key): Path<String>,
    Json(body): Json<UpdateTask>,
) -> Result<Json<Task>, ApiError> {
    let UpdateTask {
        mut change,
        saving_mode,
        autopilot,
        view,
    } = body;
    check_autopilot(autopilot, saving_mode)?;
    let view = view.as_deref().map(parse_view).transpose()?;
    if let Some(title) = &mut change.title {
        *title = title.trim().to_string();
        if title.is_empty() {
            return Err(ApiError::bad_request("a task needs a title"));
        }
    }
    let key = TaskKey::new(key);
    // A view-only change works in any state; a title alone, on any task but an archived
    // one; the rest edits the backlog.
    let title_only = change.notes.is_none()
        && change.project.is_none()
        && change.workflow.is_none()
        && change.in_checkout.is_none();
    let mut task = if view.is_some() && change.is_empty() {
        app.store
            .get_task(&key)
            .await?
            .ok_or_else(|| crate::store::StoreError::NotFound(key.clone()))?
    } else {
        match &change.title {
            Some(title) if title_only => app.store.set_title(&key, title).await?,
            _ => app.store.update_backlog_task(&key, &change).await?,
        }
    };
    if let Some(view) = view.filter(|v| *v != task.view) {
        task = app.store.set_view(&key, view).await?;
    }
    if let Some(on) = saving_mode.filter(|on| *on != task.saving_mode) {
        task = crate::handoff::apply_saving_mode(&app, &key, on).await?;
    }
    if let Some(on) = autopilot {
        if on && !crate::autopilot::allowed(&key, &task.workflow) {
            return Err(crate::autopilot::not_spec_workflow(&key));
        }
        // On brings saving mode with it; off leaves it as it is. A paused or finished
        // autopilot stays as it is (`resume` continues it), like `turn_on`.
        app.store
            .update_autopilot(&key, |current| match on {
                true => current.is_none().then(|| Some(crate::autopilot::fresh())),
                false => current.is_some().then_some(None),
            })
            .await?;
        task = app.store.get_task(&key).await?.unwrap_or(task);
    }
    app.emit(crate::events::Topic::Tasks);
    Ok(Json(task))
}

/// `PATCH /api/v1/tasks/{key}`: the backlog change (backlog only) plus saving mode,
/// autopilot and `view`, which work in any state.
#[derive(Debug, Deserialize)]
struct UpdateTask {
    #[serde(flatten)]
    change: crate::store::BacklogChange,
    #[serde(default)]
    saving_mode: Option<bool>,
    #[serde(default)]
    autopilot: Option<bool>,
    #[serde(default)]
    view: Option<String>,
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
    /// For a 422 on a form: each problem, tied to its field.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fields: Vec<delilah_core::FieldError>,
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
    pub(crate) fields: Vec<delilah_core::FieldError>,
    pub(crate) resume_at: Option<chrono::DateTime<Utc>>,
    pub(crate) reason: Option<String>,
}

impl ApiError {
    /// A 422 that names the fields at fault.
    pub fn invalid_fields(
        message: impl Into<String>,
        fields: Vec<delilah_core::FieldError>,
    ) -> Self {
        Self {
            fields,
            ..Self::unprocessable(message)
        }
    }

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
            fields: Vec::new(),
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
                fields: self.fields,
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
            claude_bin: "claude".into(),
            agent_program: None,
            ..crate::config::Config::test_defaults()
        };
        router(AppState::new(Store::in_memory().await.unwrap(), config))
    }

    #[tokio::test]
    async fn health_tells_the_home_folder() {
        let app = app().await;
        let (status, body) = call(&app, "GET", "/api/v1/health", None).await;
        assert_eq!(status, StatusCode::OK);
        let home = body["home"].as_str().unwrap();
        assert!(std::path::Path::new(home).is_absolute());
        assert!(home.contains("delilah-api-"));
    }

    #[tokio::test]
    async fn every_new_route_is_registered() {
        let app = app().await;
        let ids = Some(json!({ "ids": [] }));
        let routes = [
            ("POST", "/api/v1/tasks/T-1/reopen", None),
            ("POST", "/api/v1/tasks/T-1/prs/1/merge", None),
            ("GET", "/api/v1/tasks/T-1/setup", None),
            ("GET", "/api/v1/tasks/T-1/setup/log", None),
            ("GET", "/api/v1/tasks/T-1/base-status", None),
            ("POST", "/api/v1/tasks/T-1/update-branch", None),
            ("POST", "/api/v1/inbox/question%3AT-1%3Aa%3A1/dismiss", None),
            ("POST", "/api/v1/inbox/clear", None),
            ("POST", "/api/v1/inbox/restore", ids),
            ("GET", "/api/v1/jira/issues/T-1/subtasks", None),
            (
                "POST",
                "/api/v1/sessions/s1/runtime",
                Some(json!({ "mode": "chat" })),
            ),
        ];
        for (method, uri, body) in routes {
            // A missing route is axum's bare 404/405; a handler's own 404/409 carries
            // the ApiError JSON body.
            let (status, body) = call(&app, method, uri, body).await;
            let missing = matches!(
                status,
                StatusCode::NOT_FOUND | StatusCode::METHOD_NOT_ALLOWED
            ) && body.get("error").is_none();
            assert!(!missing, "{method} {uri}: route not registered");
        }
    }

    /// The API with a `web` repo project registered (its folder is never touched).
    async fn app_with_project() -> (Router, AppState) {
        let config = crate::config::Config {
            home: std::env::temp_dir().join(format!("delilah-api-{}", uuid::Uuid::new_v4())),
            addr: crate::config::DEFAULT_ADDR.parse().unwrap(),
            allow_file_protocol: false,
            notify: false,
            ui_origin: None,
            claude_bin: "claude".into(),
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
                settings: Default::default(),
                preview_command: None,
            })
            .await
            .unwrap();
        (router(state.clone()), state)
    }

    fn git_in(dir: &std::path::Path, args: &[&str]) {
        let out = std::process::Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(["-c", "user.name=t", "-c", "user.email=t@t"])
            .args(args)
            .output()
            .unwrap();
        assert!(out.status.success(), "git {args:?}");
    }

    /// A task `key` with a real repo as workspace, branchless (detached) when `branch`
    /// is empty.
    async fn task_in_repo(state: &AppState, key: &str, branch: &str) -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!("delilah-api-ws-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        git_in(&root, &["init", "-q", "-b", "main"]);
        git_in(&root, &["commit", "-q", "--allow-empty", "-m", "base"]);
        if branch.is_empty() {
            git_in(&root, &["checkout", "-q", "--detach"]);
        } else {
            git_in(&root, &["checkout", "-q", "-b", branch]);
        }
        let key = TaskKey::new(key);
        state
            .store
            .create_task(&Task::new(key.clone(), "Idea", "feature"))
            .await
            .unwrap();
        let ws = delilah_git::Workspace {
            path: root.clone(),
            branch: branch.into(),
            kind: delilah_core::ProjectKind::Repo,
            submodules: vec![],
        };
        state.store.save_workspace(&key, &root, &ws).await.unwrap();
        state
            .store
            .set_spec_dirs_at_start(&key, Some(&[]))
            .await
            .unwrap();
        state
            .store
            .set_started_branchless(&key, branch.is_empty())
            .await
            .unwrap();
        root
    }

    #[tokio::test]
    async fn the_branch_route_retries_a_free_task() {
        let (app, state) = app_with_project().await;
        let root = task_in_repo(&state, "LIBRE-1", "").await;
        state
            .store
            .set_branch_error(&TaskKey::new("LIBRE-1"), Some("old"))
            .await
            .unwrap();
        let folder = root.join("specs/005-y");
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join("spec.md"), "# Feature Specification: Y\n").unwrap();
        let (status, task) = call(&app, "POST", "/api/v1/tasks/LIBRE-1/branch", None).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(task["key"], "LIBRE-1");
        assert_eq!(task["branch_error"], Value::Null);
        let (_, ws) = state
            .store
            .workspace(&TaskKey::new("LIBRE-1"))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(ws.branch, "005-y");
        // Now on its branch: nothing to retry.
        let (status, _) = call(&app, "POST", "/api/v1/tasks/LIBRE-1/branch", None).await;
        assert_eq!(status, StatusCode::CONFLICT);
    }

    #[tokio::test]
    async fn the_branch_route_answers_404_and_409() {
        let (app, state) = app_with_project().await;
        let (status, _) = call(&app, "POST", "/api/v1/tasks/LIBRE-9/branch", None).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        task_in_repo(&state, "PROJ-1", "").await;
        let (status, _) = call(&app, "POST", "/api/v1/tasks/PROJ-1/branch", None).await;
        assert_eq!(status, StatusCode::CONFLICT);
        task_in_repo(&state, "LIBRE-2", "006-z").await;
        let (status, _) = call(&app, "POST", "/api/v1/tasks/LIBRE-2/branch", None).await;
        assert_eq!(status, StatusCode::CONFLICT);
    }

    #[tokio::test]
    async fn the_guard_route_allows_and_checks_the_task_name() {
        let (app, state) = app_with_project().await;
        task_in_repo(&state, "LIBRE-3", "").await;
        let read = json!({ "tool_name": "Read", "tool_input": { "file_path": "a" } });
        let (status, body) = call(
            &app,
            "POST",
            "/hooks/claude/guard?task=LIBRE-3&session=s",
            Some(read.clone()),
        )
        .await;
        assert_eq!((status, body), (StatusCode::OK, json!({})));
        let (status, _) = call(
            &app,
            "POST",
            "/hooks/claude/guard?task=..&session=s",
            Some(read),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        let edit = json!({ "tool_name": "Edit", "tool_input": { "file_path": "src/a.rs" } });
        let (status, body) = call(
            &app,
            "POST",
            "/hooks/claude/guard?task=LIBRE-3&session=s",
            Some(edit),
        )
        .await;
        assert_eq!((status, body), (StatusCode::OK, json!({})));
        let (_, ws) = state
            .store
            .workspace(&TaskKey::new("LIBRE-3"))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(ws.branch, "idea");
    }

    #[tokio::test]
    async fn the_view_is_set_at_creation_and_by_patch_in_any_state() {
        let (app, state) = app_with_project().await;
        let (status, task) = call(
            &app,
            "POST",
            "/api/v1/tasks",
            Some(json!({"free": true, "title": "a", "project": "web"})),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED, "{task}");
        assert_eq!(task["view"], "chat", "chat by default");
        let (status, task) = call(
            &app,
            "POST",
            "/api/v1/tasks",
            Some(json!({"free": true, "title": "b", "project": "web", "view": "terminal"})),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED, "{task}");
        assert_eq!(task["view"], "terminal");

        // Not a backlog task any more: the view still changes.
        let key = TaskKey::new("LIBRE-1");
        state
            .store
            .apply_event(&key, delilah_core::Event::Start, Utc::now())
            .await
            .unwrap();
        let mut rx = state.events.subscribe();
        let (status, task) = call(
            &app,
            "PATCH",
            "/api/v1/tasks/LIBRE-1",
            Some(json!({"view": "terminal"})),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{task}");
        assert_eq!(task["view"], "terminal");
        assert_eq!(rx.try_recv().unwrap(), crate::events::Topic::Tasks);
        let stored = state.store.get_task(&key).await.unwrap().unwrap();
        assert_eq!(stored.view, delilah_core::TaskView::Terminal);

        for (method, uri, body) in [
            ("PATCH", "/api/v1/tasks/LIBRE-1", json!({"view": "split"})),
            (
                "POST",
                "/api/v1/tasks",
                json!({"free": true, "title": "c", "view": "x"}),
            ),
        ] {
            let (status, body) = call(&app, method, uri, Some(body)).await;
            assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
        }
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
            Some(json!({"notes": "y"})),
        )
        .await;
        assert_eq!(
            status,
            StatusCode::CONFLICT,
            "only backlog tasks can be edited"
        );
    }

    #[tokio::test]
    async fn the_title_alone_can_change_on_a_running_task() {
        let (app, _) = app_with_project().await;
        call(
            &app,
            "POST",
            "/api/v1/tasks",
            Some(json!({"key": "PROJ-1", "title": "x", "project": "web"})),
        )
        .await;
        call(
            &app,
            "POST",
            "/api/v1/tasks/PROJ-1/events",
            Some(json!({"type": "start"})),
        )
        .await;
        let (status, task) = call(
            &app,
            "PATCH",
            "/api/v1/tasks/PROJ-1",
            Some(json!({"title": "  mine "})),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{task}");
        assert_eq!(task["title"], "mine");
        assert_eq!(task["title_source"], "manual");
        assert!(task["previous_title"].is_null());
        for body in [
            json!({"title": "z", "notes": "n"}),
            json!({"title": "z", "project": "web"}),
            json!({"title": "z", "workflow": "w"}),
            json!({"title": "z", "in_checkout": true}),
        ] {
            let (status, _) = call(&app, "PATCH", "/api/v1/tasks/PROJ-1", Some(body.clone())).await;
            assert_eq!(status, StatusCode::CONFLICT, "{body}");
        }
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
    async fn a_task_is_created_in_chat_mode_only_when_asked() {
        let app = app().await;
        let (status, task) = call(
            &app,
            "POST",
            "/api/v1/tasks",
            Some(json!({"key": "PROJ-1", "title": "Chat", "chat_mode": true})),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED);
        assert_eq!(task["chat_mode"], true);
        assert_eq!(task["allowed_tools"], json!([]));
        let (_, task) = call(
            &app,
            "POST",
            "/api/v1/tasks",
            Some(json!({"key": "PROJ-2", "title": "Plain"})),
        )
        .await;
        assert_eq!(task["chat_mode"], false);
        let (_, list) = call(&app, "GET", "/api/v1/tasks", None).await;
        assert!(
            list.as_array()
                .unwrap()
                .iter()
                .any(|t| t["chat_mode"] == true)
        );
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
    async fn setup_actions_are_refused_from_the_wrong_status() {
        let (app, state) = app_with_project().await;
        let key = TaskKey::new("PROJ-1");
        let mut task = Task::new(key.clone(), "T", "custom");
        task.project = Some("web".into());
        state.store.create_task(&task).await.unwrap();
        state.transition(&key, Event::Start).await.unwrap();
        for action in ["retry", "skip", "cancel"] {
            let (status, _) = call(
                &app,
                "POST",
                &format!("/api/v1/tasks/PROJ-1/setup/{action}"),
                None,
            )
            .await;
            assert_eq!(status, StatusCode::CONFLICT, "{action}");
        }
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
    async fn a_backlog_task_takes_autopilot_and_it_forces_saving_mode() {
        let (app, _state) = app_with_project().await;
        let new = |extra: Value| {
            let mut body =
                json!({"free": true, "title": "T", "project": "web", "workflow": "feature"});
            body.as_object_mut()
                .unwrap()
                .extend(extra.as_object().unwrap().clone());
            body
        };
        let (status, _) = call(
            &app,
            "POST",
            "/api/v1/tasks",
            Some(new(json!({"autopilot": true, "saving_mode": false}))),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        let (status, task) = call(
            &app,
            "POST",
            "/api/v1/tasks",
            Some(new(json!({"autopilot": true}))),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED);
        assert_eq!(task["saving_mode"], true);
        assert_eq!(task["autopilot"]["state"], "running");

        let (_, plain) = call(&app, "POST", "/api/v1/tasks", Some(new(json!({})))).await;
        assert!(plain["autopilot"].is_null());
        let uri = format!("/api/v1/tasks/{}", plain["key"].as_str().unwrap());
        let (status, _) = call(
            &app,
            "PATCH",
            &uri,
            Some(json!({"autopilot": true, "saving_mode": false})),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        let (_, task) = call(&app, "PATCH", &uri, Some(json!({"autopilot": true}))).await;
        assert_eq!(task["saving_mode"], true);
        assert_eq!(task["autopilot"]["state"], "running");
        let (_, task) = call(&app, "PATCH", &uri, Some(json!({"title": "U"}))).await;
        assert_eq!(task["autopilot"]["state"], "running", "left out, it stays");
        let (_, task) = call(&app, "PATCH", &uri, Some(json!({"autopilot": false}))).await;
        assert!(task["autopilot"].is_null());
        assert_eq!(task["saving_mode"], true, "off keeps saving mode");
        let (_, task) = call(&app, "PATCH", &uri, Some(json!({"autopilot": true}))).await;
        assert!(task["autopilot"].is_object());
        let (_, task) = call(&app, "PATCH", &uri, Some(json!({"saving_mode": false}))).await;
        assert!(task["autopilot"].is_null(), "saving mode off clears it");
    }

    #[tokio::test]
    async fn autopilot_is_for_the_spec_workflow_only() {
        let (app, _state) = app_with_project().await;
        let body = |extra: Value| {
            let mut b = json!({"free": true, "title": "T", "project": "web"});
            b.as_object_mut()
                .unwrap()
                .extend(extra.as_object().unwrap().clone());
            b
        };
        let (status, err) = call(
            &app,
            "POST",
            "/api/v1/tasks",
            Some(body(json!({"autopilot": true, "workflow": "prompt"}))),
        )
        .await;
        assert_eq!(status, StatusCode::CONFLICT);
        assert_eq!(err["reason"], "not-spec-workflow");
        let (_, plain) = call(
            &app,
            "POST",
            "/api/v1/tasks",
            Some(body(json!({"workflow": "prompt"}))),
        )
        .await;
        let uri = format!("/api/v1/tasks/{}", plain["key"].as_str().unwrap());
        let (status, err) = call(&app, "PATCH", &uri, Some(json!({"autopilot": true}))).await;
        assert_eq!(status, StatusCode::CONFLICT);
        assert_eq!(err["reason"], "not-spec-workflow");
        let (status, _) = call(&app, "PATCH", &uri, Some(json!({"autopilot": false}))).await;
        assert_eq!(status, StatusCode::OK, "off is always fine");
    }

    #[tokio::test]
    async fn patching_autopilot_on_keeps_a_paused_one() {
        let (app, state) = app_with_project().await;
        let (_, task) = call(
            &app,
            "POST",
            "/api/v1/tasks",
            Some(json!({"free": true, "title": "T", "project": "web", "workflow": "feature", "autopilot": true})),
        )
        .await;
        let key = TaskKey::new(task["key"].as_str().unwrap());
        let mut paused = crate::autopilot::fresh();
        paused.state = delilah_core::AutopilotState::Paused;
        state
            .store
            .set_autopilot(&key, Some(&paused))
            .await
            .unwrap();
        let uri = format!("/api/v1/tasks/{key}");
        let (_, task) = call(&app, "PATCH", &uri, Some(json!({"autopilot": true}))).await;
        assert_eq!(task["autopilot"]["state"], "paused");
    }

    #[tokio::test]
    async fn the_autopilot_route_refuses_tasks_without_saving_mode() {
        let (app, state) = app_with_project().await;
        let key = TaskKey::new("PROJ-1");
        state
            .store
            .create_task(&Task::new(key, "t", "prompt"))
            .await
            .unwrap();
        let (status, body) = call(
            &app,
            "POST",
            "/api/v1/tasks/PROJ-1/autopilot",
            Some(json!({"action": "on"})),
        )
        .await;
        assert_eq!(status, StatusCode::CONFLICT);
        assert_eq!(body["reason"], "not-spec-workflow");
    }

    #[tokio::test]
    async fn saving_mode_off_clears_autopilot_through_the_route() {
        let (app, state) = app_with_project().await;
        let key = TaskKey::new("PROJ-1");
        state
            .store
            .create_task(&Task::new(key.clone(), "t", "feature"))
            .await
            .unwrap();
        let (status, task) = call(
            &app,
            "POST",
            "/api/v1/tasks/PROJ-1/autopilot",
            Some(json!({"action": "on"})),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(task["autopilot"]["state"], "running");
        assert_eq!(task["saving_mode"], true);
        let (_, task) = call(
            &app,
            "POST",
            "/api/v1/tasks/PROJ-1/saving-mode",
            Some(json!({"on": false})),
        )
        .await;
        assert!(task["autopilot"].is_null());
        assert_eq!(task["saving_mode"], false);
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
            claude_bin: "claude".into(),
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
            claude_bin: "claude".into(),
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
