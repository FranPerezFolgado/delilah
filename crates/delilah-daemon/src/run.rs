//! `POST /api/v1/runs`: creates a task, builds its workspace and starts the agent.

use std::sync::Arc;
use std::time::Duration;

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use delilah_agents::claude;
use delilah_agents::pty::PtySession;
use delilah_agents::runtime::{self, Runtime, RuntimeKind};
use delilah_core::{Event, ProjectKind, Step, Task, TaskKey, TaskStatus};
use delilah_git::{Deadline, FetchOutcome, GitOptions, RepoBase, WorkspaceSpec};
use serde::Deserialize;
use tokio::sync::broadcast::error::RecvError;

use crate::api::ApiError;
use crate::events::Topic;
use crate::state::AppState;
use crate::store::BacklogChange;
use crate::store_prs::NewNotice;
use crate::store_sessions::{SessionKind, SessionRecord, project_of};

/// How long a new session may show the folder-trust dialog before we stop watching.
const TRUST_WATCH: Duration = Duration::from_secs(60);

/// Added to the system prompt of an idea conversation.
const IDEA_PROMPT: &str = "This is a Delilah idea conversation, shown in a chat view, not a terminal. Follow the task-from-idea skill from the first message. Ask with AskUserQuestion. Never use ExitPlanMode.";

use crate::store_prs::KIND_BASE_STALE;

/// Logs where each repo's task branch started (the ref and the commit), and warns in
/// the inbox for each repo whose fetch failed. Never fails the start. The warning's
/// cause is the moment of the start (a branchless task has no branch yet), so a
/// warning dismissed at one start still shows at the next.
async fn report_bases(app: &AppState, key: &TaskKey, project: &str, bases: &[RepoBase]) {
    let started = format!("start {}", chrono::Utc::now().to_rfc3339());
    let mut warned = false;
    for b in bases {
        tracing::info!(task = %key, repo = %b.repo, base = %b.refname, sha = b.sha.as_deref().unwrap_or("-"), fetch = ?b.fetch, "task branch base");
        let FetchOutcome::Failed(reason) = &b.fetch else {
            continue;
        };
        let reason = short_reason(reason);
        let reason = reason.as_str();
        let name = if b.repo == "." { project } else { &b.repo };
        let sha7: String = b.start().chars().take(7).collect();
        let text = format!(
            "{} {name} ({reason}); {} {} @ {sha7} {}",
            app.lang().pick("No se pudo actualizar", "Could not update"),
            app.lang()
                .pick("la rama empieza en", "the branch starts at"),
            b.refname,
            app.lang()
                .pick("y puede ir atrasada.", "and may be behind."),
        );
        let notice = NewNotice {
            repo: b.repo.clone(),
            kind: KIND_BASE_STALE.into(),
            cause: started.clone(),
            text,
            detail: Some(serde_json::json!({
                "refname": b.refname, "sha": b.sha, "reason": reason
            })),
        };
        match app.store.insert_notice(key, &notice).await {
            Ok(new) => warned |= new,
            Err(e) => tracing::warn!(task = %key, "cannot record the stale base: {e}"),
        }
    }
    if warned {
        app.emit(Topic::Inbox);
    }
}

/// Git's stderr cut to what a notice can show: the first `fatal:`/`error:` line (else
/// the first non-empty one), no control characters, no credentials in URLs, at most
/// 200 chars.
fn short_reason(stderr: &str) -> String {
    let mut lines = stderr.lines().map(str::trim).filter(|l| !l.is_empty());
    let line = lines
        .clone()
        .find(|l| l.starts_with("fatal:") || l.starts_with("error:"))
        .or_else(|| lines.next())
        .unwrap_or("");
    let mut out = String::new();
    let mut rest = line;
    // `scheme://user:secret@host` → `scheme://host`
    while let Some(at) = rest.find("://") {
        out.push_str(&rest[..at + 3]);
        rest = &rest[at + 3..];
        let end = rest
            .find(|c: char| c.is_whitespace() || c == '/' || c == '\'' || c == '"')
            .unwrap_or(rest.len());
        if let Some(cred) = rest[..end].rfind('@') {
            rest = &rest[cred + 1..];
        }
    }
    out.push_str(rest);
    out.chars().filter(|c| !c.is_control()).take(200).collect()
}

/// How a Claude session is launched, by what it is: a conversation never asks
/// (`DontAsk`) and gets the Delilah plugin and the idea prompt; a read-only one plans;
/// the plugin (which can create tasks) goes only to project sessions, never to a
/// task's agents. Task and project sessions follow the `agent-permissions` preference
/// (`Auto` or Claude's default); the idea and read-only ones ignore it. Pure: the
/// preference arrives as an argument.
fn launch_flags(
    session: &SessionRecord,
    config: &crate::config::Config,
    agent_permissions: crate::state::AgentPermissions,
) -> (
    claude::Permission,
    Option<std::path::PathBuf>,
    Option<&'static str>,
) {
    let permission = if session.conversation {
        claude::Permission::DontAsk
    } else if session.read_only {
        claude::Permission::Plan
    } else {
        match agent_permissions {
            crate::state::AgentPermissions::Auto => claude::Permission::Auto,
            crate::state::AgentPermissions::Default => claude::Permission::Default,
        }
    };
    let plugin_dir = project_of(&session.task)
        .is_some()
        .then(|| config.plugin_dir());
    let prompt = session.conversation.then_some(IDEA_PROMPT);
    (permission, plugin_dir, prompt)
}

#[derive(Debug, Deserialize)]
pub struct RunRequest {
    pub key: String,
    /// Left out for a backlog task: its own.
    #[serde(default)]
    pub title: String,
    /// Name of a registered project. Left out for a backlog task: its own.
    #[serde(default)]
    pub project: String,
    /// What the agent gets with the task (`""` clears a backlog task's). Left out
    /// for a backlog task: its own.
    #[serde(default)]
    pub notes: Option<String>,
    /// Defaults to the project's workflow.
    pub workflow: Option<String>,
    /// `parent` projects only: the submodules the task touches.
    #[serde(default)]
    pub submodules: Vec<String>,
    /// Defaults to the project's branch template if it has one; else a free task starts
    /// without a branch (decision 65) and a Jira task gets a name Claude proposes from
    /// the title, as `feature/KEY-…`. Always normalised and made unique.
    pub branch: Option<String>,
    /// Defaults to `/feature <key>` for the `feature` workflow (`/feature <title>`
    /// for a free task).
    pub prompt: Option<String>,
    /// A task without a Jira ticket: `key` may be empty, Delilah names it `LIBRE-<n>`.
    #[serde(default)]
    pub free: bool,
    /// Work in the project's own checkout (a branch there) instead of a worktree.
    /// Left out: what a backlog task chose, else the project's default workspace, else the checkout.
    #[serde(default)]
    pub in_checkout: Option<bool>,
    /// Saving mode: each phase runs in a fresh session. Left out: what a backlog
    /// task chose, else the `saving-mode` preference.
    #[serde(default)]
    pub saving_mode: Option<bool>,
    /// Autopilot: the chain runs by itself until the PRs are open. Implies saving
    /// mode. Left out: what a backlog task chose, else off.
    #[serde(default)]
    pub autopilot: Option<bool>,
    /// A Claude session of the project to carry on as the task's primary one, with
    /// its conversation ("Convertir en tarea"). Implies a free task in the checkout.
    pub from_session: Option<String>,
    /// Chat mode: the agent runs on a stream-json pipe, shown as a chat. Left out:
    /// what a backlog task chose, else off.
    #[serde(default)]
    pub chat_mode: Option<bool>,
    /// CI autofix on or off. Left out: what a backlog task chose, else the project's
    /// default (`always`, `autopilot` or `off`).
    #[serde(default)]
    pub ci_autofix: Option<bool>,
}

pub async fn start_run(
    State(app): State<AppState>,
    Json(req): Json<RunRequest>,
) -> Result<(StatusCode, Json<Task>), ApiError> {
    start_run_with(app, req, FIRST_TURN_TIMEOUT, SETTLE).await
}

/// [`start_run`] with the waits of a chat-mode start given (tests shorten or stretch them).
pub(crate) async fn start_run_with(
    app: AppState,
    mut req: RunRequest,
    first_turn_timeout: Duration,
    settle: Duration,
) -> Result<(StatusCode, Json<Task>), ApiError> {
    // The conversation lives with the folder it was held in: carrying it over means
    // working in that same checkout.
    let adopted = match &req.from_session {
        Some(id) => {
            let s = app
                .store
                .session(id)
                .await?
                .filter(|s| !s.closed && s.kind == SessionKind::Claude)
                .ok_or_else(|| ApiError::not_found(format!("no open Claude session {id}")))?;
            if crate::store_sessions::project_of(&s.task) != Some(req.project.as_str()) {
                return Err(ApiError::bad_request(format!(
                    "session {id} is not one of {}'s questions",
                    req.project
                )));
            }
            req.free = true;
            req.in_checkout = Some(true);
            Some(s)
        }
        None => None,
    };
    if req.free && req.key.trim().is_empty() {
        req.key = next_free_key(&app).await?;
    }
    let key = TaskKey::parse(&req.key).map_err(|e| ApiError::bad_request(e.to_string()))?;
    // A backlog task is started as it is; any other existing task already was.
    let existing = match app.store.get_task(&key).await? {
        Some(task) if task.status == TaskStatus::Queued => Some(task),
        Some(task) => {
            return Err(ApiError::conflict(format!(
                "{key} is already {}: only a backlog task can be started",
                task.status.name()
            )));
        }
        None => None,
    };
    if let Some(task) = &existing {
        if req.title.trim().is_empty() {
            req.title = task.title.clone();
        }
        if req.project.trim().is_empty() {
            req.project = task.project.clone().unwrap_or_default();
        }
    }
    let saving_mode = match (req.saving_mode, &existing) {
        (Some(on), _) => on,
        (None, Some(task)) => task.saving_mode,
        (None, None) => saving_mode_preference(&app).await,
    };
    if req.autopilot == Some(true) && req.saving_mode == Some(false) {
        return Err(ApiError::bad_request(
            "autopilot needs saving mode: it cannot run with saving mode off",
        ));
    }
    // Autopilot on implies saving mode; saving mode off turns an inherited one off.
    let saving_mode = saving_mode || req.autopilot == Some(true);
    let autopilot = saving_mode
        && match (req.autopilot, &existing) {
            (Some(on), _) => on,
            (None, Some(task)) => task.autopilot.is_some(),
            (None, None) => false,
        };
    if req.title.trim().is_empty() {
        return Err(ApiError::bad_request(match req.free {
            true => "say what the free task is about",
            false => "a new task needs a title",
        }));
    }
    if req.project.trim().is_empty() {
        return Err(ApiError::bad_request(
            "say which project the task runs against",
        ));
    }
    let project = app.store.project(&req.project).await?;
    // The request wins, then what the backlog task chose, then the project's setting;
    // with none of them, the checkout (decision 57 reversed).
    let in_checkout = req
        .in_checkout
        .or_else(|| existing.as_ref().and_then(|t| t.in_checkout))
        .or(project
            .settings
            .workspace
            .map(|m| m == delilah_core::WorkspaceMode::Checkout))
        .unwrap_or(true);
    let rules = &project.settings.submodules;
    if project.kind == ProjectKind::Parent && !(rules.always.is_empty() && rules.never.is_empty()) {
        let root = project.path.clone();
        let available: Vec<String> =
            tokio::task::spawn_blocking(move || delilah_git::list_submodules(&root))
                .await
                .map_err(|e| ApiError::internal(e.to_string()))?
                .map_err(|e| ApiError::unprocessable(e.to_string()))?
                .into_iter()
                .map(|s| s.path)
                .collect();
        match effective_submodules(&req.submodules, rules, &available, app.lang()) {
            Ok(merged) => req.submodules = merged,
            Err(fields) => {
                let summary = fields
                    .iter()
                    .map(|f| f.message.as_str())
                    .collect::<Vec<_>>()
                    .join(" ");
                return Err(ApiError::invalid_fields(summary, fields));
            }
        }
    }
    match (project.kind, req.submodules.is_empty()) {
        (ProjectKind::Parent, true) => {
            return Err(ApiError::bad_request(format!(
                "{} is a parent project: say which --submodules the task touches",
                project.name
            )));
        }
        (ProjectKind::Repo, false) => {
            return Err(ApiError::bad_request(format!(
                "{} is a repo project: it has no --submodules",
                project.name
            )));
        }
        _ => {}
    }
    let workflow = req
        .workflow
        .clone()
        .or_else(|| existing.as_ref().map(|t| t.workflow.clone()))
        .unwrap_or_else(|| project.workflow.clone());
    if req.autopilot == Some(true) && !crate::autopilot::allowed(&key, &workflow) {
        return Err(crate::autopilot::not_spec_workflow(&key));
    }
    let prompt = match (&req.prompt, workflow.as_str()) {
        (Some(prompt), _) => prompt.clone(),
        (None, "feature") if req.free => format!("/feature {}", req.title.trim()),
        (None, "feature") => format!("/feature {}", req.key),
        (None, other) => {
            return Err(ApiError::bad_request(format!(
                "workflow `{other}` needs a --prompt"
            )));
        }
    };

    // Named before anything is created, so a refused name leaves a backlog task as it is.
    let notes = req
        .notes
        .clone()
        .or_else(|| existing.as_ref().and_then(|t| t.notes.clone()));
    // Precedence at start: a typed branch, then the project's template, then (free
    // task) no branch at all, else a proposal.
    let typed = req
        .branch
        .as_deref()
        .map(str::trim)
        .is_some_and(|b| !b.is_empty());
    let branchless = crate::branching::starts_branchless(&project, &key, typed);
    let branch = match branchless {
        true => None,
        false => Some(
            crate::branch_name::for_start(
                &app,
                &project,
                &req.key,
                &req.title,
                notes.as_deref(),
                req.free,
                &req.submodules,
                req.branch.as_deref(),
            )
            .await?,
        ),
    };

    if in_checkout {
        match app.store.checkout_user(&project.path).await? {
            Some((other, false)) => {
                return Err(ApiError::conflict(format!(
                    "{other} is still working in {}'s checkout: mark it done first, or use a worktree",
                    project.name
                )));
            }
            // A finished task no longer needs the checkout: release it as it is (its
            // branch stays; the folder stays on whatever branch it is on now).
            Some((other, true)) => {
                app.store.delete_workspace(&TaskKey::new(&other)).await?;
                tracing::info!(task = %other, project = %project.name, "released the checkout of a finished task");
            }
            None => {}
        }
    }

    let notes = match &existing {
        Some(_) => {
            // It starts where and as it is asked to now.
            let change = BacklogChange {
                title: Some(req.title.trim().to_string()),
                project: Some(project.name.clone()),
                workflow: Some(workflow.clone()),
                notes: req.notes.clone(),
                in_checkout: Some(in_checkout),
            };
            let updated = app.store.update_backlog_task(&key, &change).await?;
            if updated.saving_mode != saving_mode {
                app.store.set_saving_mode(&key, saving_mode).await?;
            }
            if let Some(chat) = req.chat_mode.filter(|&c| c != updated.chat_mode) {
                app.store.set_chat_mode(&key, chat).await?;
            }
            if updated.autopilot.is_some() != autopilot {
                let value = autopilot.then(new_autopilot);
                app.store.set_autopilot(&key, value.as_ref()).await?;
            }
            // A backlog task keeps its own switch unless the request says otherwise.
            if let Some(on) = req.ci_autofix.filter(|&on| on != updated.ci_autofix) {
                app.store.set_ci_autofix(&key, on).await?;
            }
            updated.notes
        }
        None => {
            let mut task = Task::new(key.clone(), req.title.trim(), &workflow);
            task.project = Some(project.name.clone());
            task.saving_mode = saving_mode;
            task.autopilot = autopilot.then(new_autopilot);
            task.chat_mode = req.chat_mode.unwrap_or(false);
            task.ci_autofix = crate::api::resolve_ci_autofix(
                req.ci_autofix,
                project.settings.ci_autofix,
                autopilot,
            );
            task.notes = req.notes.clone().filter(|n| !n.trim().is_empty());
            if crate::branching::is_free(&key) {
                task.title_source = delilah_core::TitleSource::Initial;
            }
            app.store.create_task(&task).await?;
            task.notes
        }
    };
    app.transition(&key, Event::Start).await?;

    let spec = WorkspaceSpec {
        path: app.config.workspaces_dir().join(&req.key),
        // Only a free task with nothing typed and no template starts without a branch:
        // the first analysis creates it (decision 65).
        branch,
        kind: project.kind,
        submodules: req.submodules.clone(),
    };
    let opts = GitOptions {
        allow_file_protocol: app.config.allow_file_protocol,
    };
    let root = project.path.clone();
    let deadline = Deadline::after(delilah_git::NETWORK_LIMIT);
    let created = tokio::task::spawn_blocking(move || {
        if in_checkout {
            delilah_git::use_checkout(&root, &spec, &opts, deadline)
                .map(|(ws, prev, bases)| (ws, Some(prev), bases))
        } else {
            delilah_git::create_workspace(&root, &spec, &opts, deadline)
                .map(|(ws, bases)| (ws, None, bases))
        }
    })
    .await
    .map_err(|e| ApiError::internal(e.to_string()))?;
    let (ws, previous, bases) = match created {
        Ok(created) => created,
        Err(e) => {
            let reason = format!("workspace: {e}");
            let fresh = existing.is_none();
            // A checkout with work in it is the user's to resolve, not an invalid request.
            let dirty = in_checkout && matches!(e, delilah_git::GitError::Dirty { .. });
            if dirty && fresh {
                // Nothing started: the task made for this request goes, not an errored one.
                app.store.delete_task(&key).await?;
            } else {
                abort_start(&app, &key, existing, false, &reason).await?;
            }
            if dirty {
                return Err(ApiError::conflict_reason(reason, "checkout-dirty"));
            }
            return Err(ApiError::unprocessable(reason));
        }
    };
    app.store.save_workspace(&key, &project.path, &ws).await?;
    if let Some(previous) = &previous {
        app.store.set_checkout_previous(&key, previous).await?;
    }
    report_bases(&app, &key, &project.name, &bases).await;
    let launch = AgentLaunch {
        free: req.free,
        prompt,
        custom_prompt: req.prompt.is_some(),
        notes,
        adopted,
        saving_mode,
        autopilot,
        existing,
        first_turn_timeout,
        settle,
    };

    // A new worktree gets the shared entries and then the setup; the project's own
    // checkout gets neither.
    let setup = match previous {
        Some(_) => {
            record_start_state(&app, &key, &ws, branchless).await?;
            None
        }
        None => {
            let (checkout, worktree) = (project.path.clone(), ws.path.clone());
            let shared = project.settings.shared.clone();
            let notes = tokio::task::spawn_blocking(move || {
                delilah_git::share_entries(&checkout, &worktree, &shared)
            })
            .await
            .map_err(|e| ApiError::internal(e.to_string()))?;
            log_share_notes(&app, &key, &notes);
            record_start_state(&app, &key, &ws, branchless).await?;
            project
                .settings
                .setup
                .clone()
                .filter(|c| !c.trim().is_empty())
        }
    };
    if let Some(command) = setup {
        // The task stays `preparing-workspace`: the setup's end makes it ready and
        // launches the agent.
        if let Err(e) = crate::setup::start(&app, &key, &command, launch).await {
            let _ = app
                .transition(
                    &key,
                    Event::Fail {
                        reason: format!("setup: {e}"),
                    },
                )
                .await;
            return Err(e.into());
        }
    } else {
        start_agent(&app, &key, &ws, launch).await?;
    }

    let task = app
        .store
        .get_task(&key)
        .await?
        .expect("task was just created");
    Ok((StatusCode::CREATED, Json(task)))
}

/// What a free task's first analysis is told apart from: the spec folders present at
/// start (taken after the shared entries are linked, so a shared or generated folder
/// does not look new; the setup adds to it when it ends), and whether it started
/// without a branch. Nothing for a Jira task.
async fn record_start_state(
    app: &AppState,
    key: &TaskKey,
    ws: &delilah_git::Workspace,
    branchless: bool,
) -> Result<(), ApiError> {
    if !crate::branching::is_free(key) {
        return Ok(());
    }
    crate::branching::record_baseline(app, key, ws)
        .await
        .map_err(ApiError::internal)?;
    app.store.set_started_branchless(key, branchless).await?;
    Ok(())
}

/// One line per shared entry in the task's `workspace.log` (only if there were any).
fn log_share_notes(app: &AppState, key: &TaskKey, notes: &[delilah_git::ShareNote]) {
    use std::io::Write;
    if notes.is_empty() {
        return;
    }
    for note in notes {
        tracing::info!(task = %key, "shared entries: {note}");
    }
    let dir = app.config.task_dir(key.as_str());
    let written = std::fs::create_dir_all(&dir).and_then(|_| {
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(dir.join("workspace.log"))?;
        notes.iter().try_for_each(|n| writeln!(file, "{n}"))
    });
    if let Err(e) = written {
        tracing::warn!(task = %key, "cannot write workspace.log: {e}");
    }
}

/// The one way a task's agent is started, from a request or when a setup ends. Tests
/// swap it for a stub that only records the session: they must not start a real
/// `claude`.
pub(crate) async fn start_agent(
    app: &AppState,
    key: &TaskKey,
    ws: &delilah_git::Workspace,
    launch: AgentLaunch,
) -> Result<(), LaunchError> {
    // A test that names an `agent_program` (a fake `claude`) wants the real launch.
    #[cfg(test)]
    if app.config.agent_program.is_none() {
        let _ = (ws, launch);
        ready(app, key).await?;
        app.store
            .create_session(&SessionRecord {
                id: uuid::Uuid::new_v4().to_string(),
                task: key.to_string(),
                kind: SessionKind::Claude,
                label: "Main".into(),
                primary: true,
                closed: false,
                read_only: false,
                conversation: false,
                step: None,
            })
            .await?;
        return Ok(());
    }
    launch_agent(app, key, ws, launch).await
}

/// Why a session could not be started.
#[derive(thiserror::Error)]
pub enum LaunchError {
    #[error(transparent)]
    Store(#[from] crate::store::StoreError),
    #[error("{0}")]
    Start(String),
    /// A kind of session that is not started this way.
    #[error("{0}")]
    Unsupported(&'static str),
    /// The start failed with an answer already worded for the client.
    #[error("{}", .0.message)]
    Api(ApiError),
}

impl std::fmt::Debug for LaunchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Store(e) => f.debug_tuple("Store").field(e).finish(),
            Self::Start(m) => f.debug_tuple("Start").field(m).finish(),
            Self::Unsupported(m) => f.debug_tuple("Unsupported").field(m).finish(),
            Self::Api(e) => f.debug_tuple("Api").field(&e.message).finish(),
        }
    }
}

impl From<ApiError> for LaunchError {
    fn from(e: ApiError) -> Self {
        Self::Api(e)
    }
}

impl From<LaunchError> for ApiError {
    fn from(e: LaunchError) -> Self {
        match e {
            LaunchError::Store(e) => e.into(),
            LaunchError::Start(message) => ApiError::internal(message),
            LaunchError::Unsupported(message) => ApiError::conflict(message),
            LaunchError::Api(e) => e,
        }
    }
}

/// What `launch_agent` needs besides the task and its workspace.
pub(crate) struct AgentLaunch {
    /// A task without a Jira ticket: nothing to fetch.
    pub free: bool,
    /// The prompt as resolved (the request's own, or the workflow's default).
    pub prompt: String,
    /// The request gave its own prompt: no ticket pointer is added to it.
    pub custom_prompt: bool,
    pub notes: Option<String>,
    /// A session of the project to carry on as the primary one.
    pub adopted: Option<SessionRecord>,
    /// Saving mode: the first session is the plan step.
    pub saving_mode: bool,
    /// Autopilot answers the workflow's questions itself.
    pub autopilot: bool,
    /// The backlog task as it was before this start, to put it back if the agent does
    /// not come up. Not kept with the saved launch.
    pub existing: Option<Task>,
    /// How long a chat-mode agent has to show its first sign of work.
    pub first_turn_timeout: Duration,
    /// How long a chat-mode agent with no first message must stay alive to count as up.
    pub settle: Duration,
}

/// The launch as kept in the task dir (the session is stored by id only).
#[derive(serde::Serialize, serde::Deserialize)]
struct SavedLaunch {
    free: bool,
    prompt: String,
    custom_prompt: bool,
    notes: Option<String>,
    /// Id of the project's Claude session being carried on, if any.
    adopted: Option<String>,
    #[serde(default)]
    saving_mode: bool,
    #[serde(default)]
    autopilot: bool,
}

impl From<&AgentLaunch> for SavedLaunch {
    fn from(l: &AgentLaunch) -> Self {
        Self {
            free: l.free,
            prompt: l.prompt.clone(),
            custom_prompt: l.custom_prompt,
            notes: l.notes.clone(),
            adopted: l.adopted.as_ref().map(|s| s.id.clone()),
            saving_mode: l.saving_mode,
            autopilot: l.autopilot,
        }
    }
}

/// What the launch needs, kept so retry and skip work after a daemon restart (the
/// prompt is not stored on the task).
pub(crate) const LAUNCH_FILE: &str = "launch.json";

/// Saves the launch while the task waits for its setup.
pub(crate) fn save_launch(
    app: &AppState,
    key: &TaskKey,
    launch: &AgentLaunch,
) -> Result<(), LaunchError> {
    let dir = app.config.task_dir(key.as_str());
    std::fs::create_dir_all(&dir)
        .and_then(|_| {
            std::fs::write(
                dir.join(LAUNCH_FILE),
                serde_json::to_vec(&SavedLaunch::from(launch))?,
            )
        })
        .map_err(|e| LaunchError::Start(format!("cannot save the launch: {e}")))
}

/// The saved launch. An adopted session that is gone is dropped: the agent then
/// starts as a new primary session.
pub(crate) async fn load_launch(app: &AppState, key: &TaskKey) -> Result<AgentLaunch, LaunchError> {
    let path = app.config.task_dir(key.as_str()).join(LAUNCH_FILE);
    let bytes = std::fs::read(&path).map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => LaunchError::Unsupported("the task has no saved launch"),
        _ => LaunchError::Start(format!("cannot read the saved launch: {e}")),
    })?;
    let saved: SavedLaunch = serde_json::from_slice(&bytes)
        .map_err(|e| LaunchError::Start(format!("cannot read the saved launch: {e}")))?;
    let adopted = match &saved.adopted {
        Some(id) => app.store.session(id).await?,
        None => None,
    };
    Ok(AgentLaunch {
        free: saved.free,
        prompt: saved.prompt,
        custom_prompt: saved.custom_prompt,
        notes: saved.notes,
        adopted,
        saving_mode: saved.saving_mode,
        autopilot: saved.autopilot,
        existing: None,
        first_turn_timeout: FIRST_TURN_TIMEOUT,
        settle: SETTLE,
    })
}

/// Drops the saved launch (the agent started, or the task was archived).
pub(crate) fn forget_launch(app: &AppState, key: &TaskKey) {
    let _ = std::fs::remove_file(app.config.task_dir(key.as_str()).join(LAUNCH_FILE));
}

/// Everything after the workspace is ready: the ticket and notes the agent is
/// pointed at, then its primary session (an adopted one or a new one). Owns what it
/// needs, so it can run from a background task as well as from the request.
pub(crate) async fn launch_agent(
    app: &AppState,
    key: &TaskKey,
    ws: &delilah_git::Workspace,
    launch: AgentLaunch,
) -> Result<(), LaunchError> {
    let AgentLaunch {
        free,
        prompt,
        custom_prompt,
        notes,
        adopted,
        saving_mode,
        autopilot,
        existing,
        first_turn_timeout,
        settle,
    } = launch;
    // From the default workflow, hand the agent the ticket itself: it may not be able
    // to read Jira on its own. Kept outside the repo, so the workspace stays clean.
    let issue = match free {
        true => None,
        false => crate::jira::fetch_issue(app, &key.to_string()).await,
    };
    let prompt = match (&custom_prompt, issue) {
        (false, Some(issue)) => {
            let path = app.config.task_dir(key.as_str()).join("ticket.md");
            match crate::jira::write_ticket(&path, &issue, app.lang()) {
                Ok(()) => format!(
                    "{prompt} ({} {})",
                    app.lang()
                        .pick("el ticket de Jira está en", "the Jira ticket is at"),
                    path.display()
                ),
                Err(e) => {
                    tracing::warn!(task = %key, "cannot write the ticket: {e}");
                    prompt
                }
            }
        }
        _ => prompt,
    };
    let prompt = point_to_notes(app, key, prompt, notes.as_deref());
    let prompt = match !ws.has_branch() {
        true => format!(
            "{prompt} ({})",
            app.lang().pick(
                "La tarea aún no tiene rama: deja que Spec Kit la cree al especificar",
                "The task has no branch yet: let Spec Kit create it when specifying"
            )
        ),
        false => prompt,
    };

    // A free task that starts without a branch gets one before anything lands (R3).
    let guard = crate::branching::is_free(key) && !ws.has_branch();
    // The launch (which, in chat mode, waits up to a minute for the agent's first sign of
    // work) and its rollback run in a task of their own: a client that gives up on the
    // request drops this handler's future, and must not leave a live agent behind with the
    // task running.
    let run = Launch {
        app: app.clone(),
        key: key.clone(),
        workspace: ws.path.clone(),
        saving_mode,
        autopilot,
        first_turn_timeout,
        settle,
        guard,
    };
    tokio::spawn(run.run(adopted, existing, key.to_string(), prompt))
        .await
        .map_err(|e| LaunchError::Start(format!("the start was interrupted: {e}")))??;
    Ok(())
}

/// The workspace is ready: the task runs. A no-op when the setup already did it.
async fn ready(app: &AppState, key: &TaskKey) -> Result<(), ApiError> {
    let preparing = app
        .store
        .get_task(key)
        .await?
        .is_some_and(|t| t.status == TaskStatus::PreparingWorkspace);
    if preparing {
        app.transition(key, Event::WorkspaceReady).await?;
    }
    Ok(())
}

/// What the second half of a start needs: the agent's spawn, its first message and, when
/// either fails, the undoing of the whole start.
struct Launch {
    /// The session starts with the branch guard hook (a free task with no branch yet).
    guard: bool,
    app: AppState,
    key: TaskKey,
    workspace: std::path::PathBuf,
    saving_mode: bool,
    autopilot: bool,
    first_turn_timeout: Duration,
    settle: Duration,
}

impl Launch {
    async fn run(
        self,
        adopted: Option<SessionRecord>,
        existing: Option<Task>,
        req_key: String,
        prompt: String,
    ) -> Result<(), ApiError> {
        let Self {
            guard,
            app,
            key,
            workspace,
            saving_mode,
            autopilot,
            first_turn_timeout,
            settle,
        } = self;
        let (session, restore, launch_prompt, resume) = match adopted {
            Some(original) => {
                // The project's Claude becomes the task's: it goes on with its conversation,
                // now able to edit, on the task's branch.
                let mut session = original.clone();
                if let Some(live) = app.sessions.get(&session.id) {
                    let _ = live.runtime.kill();
                    live.runtime.wait().await;
                }
                let label = app.lang().pick("Principal", "Main");
                let adopted = app.store.adopt_session(&session.id, &req_key, label).await;
                session.task = req_key.clone();
                session.primary = true;
                session.read_only = false;
                session.label = label.into();
                if let Err(e) = adopted {
                    let reason = e.to_string();
                    rollback_start(&app, &key, &session, Some(&original), existing, &reason).await;
                    return Err(e.into());
                }
                (session, Some(original), None, true)
            }
            None => {
                let (session, prompt) =
                    first_session(&app, &req_key, saving_mode, autopilot, prompt);
                if let Err(e) = app.store.create_session(&session).await {
                    let reason = e.to_string();
                    rollback_start(&app, &key, &session, None, existing, &reason).await;
                    return Err(e.into());
                }
                (session, None, Some(prompt), false)
            }
        };
        // A chat-mode agent can fail to come up, and the start is then undone: it is spawned,
        // then the task is running, then it gets its first message (so the hooks it fires land
        // on a running task, and until then the start can still be undone). A terminal agent
        // has its prompt on the command line and, as ever, a task that runs even if it fails
        // to spawn: its session is there to reopen.
        let chat = match runtime_kind(&app, &session).await {
            Ok(kind) => kind == RuntimeKind::Stream,
            Err(e) => {
                rollback_start(&app, &key, &session, restore.as_ref(), existing, &e.message).await;
                return Err(e);
            }
        };
        if !chat {
            ready(&app, &key).await?;
        }
        let launched = async {
            let spawned = spawn_session(
                &app,
                &session,
                &workspace,
                launch_prompt.as_deref(),
                resume,
                first_turn_timeout,
                settle,
                guard,
            )
            .await?;
            if chat && let Err(e) = ready(&app, &key).await {
                spawned.discard(&app).await;
                return Err(e);
            }
            spawned.send_first(&app).await
        }
        .await;
        if let Err(e) = launched {
            if chat {
                rollback_start(&app, &key, &session, restore.as_ref(), existing, &e.message).await;
            }
            return Err(e);
        }
        if restore.is_some() {
            app.emit(crate::events::Topic::Sessions);
        }
        Ok(())
    }
}

/// Most of the agent's stderr that goes into the error of a failed start, and the longest line.
const START_ERROR_MAX: usize = 2048;
const START_ERROR_LINE_MAX: usize = 300;

/// The agent's last stderr lines as they may be shown: long lines cut, API keys (`sk-…`)
/// and bearer tokens hidden, the whole kept to its last [`START_ERROR_MAX`] bytes.
fn start_error_tail(tail: &str) -> String {
    let lines: Vec<String> = tail
        .lines()
        .map(|line| redact_secrets(&line.chars().take(START_ERROR_LINE_MAX).collect::<String>()))
        .collect();
    let joined = lines.join("\n");
    if joined.len() <= START_ERROR_MAX {
        return joined;
    }
    let mut from = joined.len() - START_ERROR_MAX;
    while !joined.is_char_boundary(from) {
        from += 1;
    }
    joined[from..].to_string()
}

fn redact_secrets(line: &str) -> String {
    let mut out: Vec<String> = Vec::new();
    let mut after_bearer = false;
    for word in line.split(' ') {
        let redacted = if after_bearer && !word.is_empty() {
            "***".to_string()
        } else if let Some(i) = word.find("sk-") {
            format!("{}sk-***", &word[..i])
        } else {
            word.to_string()
        };
        after_bearer = word.eq_ignore_ascii_case("bearer");
        out.push(redacted);
    }
    out.join(" ")
}

/// A start that did not get an agent: a backlog task goes back to the backlog as it was,
/// to be tried again; any other task fails.
///
/// `keep_new`: a task created by this very start stays in the backlog too (instead of
/// failing) so the start can be tried again, as when its agent did not come up.
async fn abort_start(
    app: &AppState,
    key: &TaskKey,
    existing: Option<Task>,
    keep_new: bool,
    reason: &str,
) -> Result<(), ApiError> {
    // Only a start in progress can go back to the backlog: the workspace being prepared,
    // or a task that already runs because its agent was given a chance (and failed on the
    // first message).
    let in_progress = app.store.get_task(key).await?.is_some_and(|t| {
        matches!(
            t.status,
            TaskStatus::PreparingWorkspace | TaskStatus::Running | TaskStatus::WaitingForUser
        )
    });
    match existing {
        None if in_progress && keep_new => {
            app.transition(
                key,
                Event::StartAborted {
                    reason: reason.to_string(),
                },
            )
            .await?;
        }
        Some(task) if in_progress => {
            app.transition(
                key,
                Event::StartAborted {
                    reason: reason.to_string(),
                },
            )
            .await?;
            let undo = BacklogChange {
                title: Some(task.title),
                project: task.project,
                workflow: Some(task.workflow),
                notes: Some(task.notes.unwrap_or_default()),
                in_checkout: task.in_checkout,
            };
            if let Err(e) = app.store.update_backlog_task(key, &undo).await {
                tracing::warn!(task = %key, "cannot put it back as it was: {e}");
            }
            if let Err(e) = app.store.set_saving_mode(key, task.saving_mode).await {
                tracing::warn!(task = %key, "cannot put saving mode back: {e}");
            }
            if let Err(e) = app.store.set_autopilot(key, task.autopilot.as_ref()).await {
                tracing::warn!(task = %key, "cannot put autopilot back: {e}");
            }
            if let Err(e) = app.store.set_chat_mode(key, task.chat_mode).await {
                tracing::warn!(task = %key, "cannot put chat mode back: {e}");
            }
        }
        _ => {
            app.transition(
                key,
                Event::Fail {
                    reason: reason.to_string(),
                },
            )
            .await?;
        }
    }
    Ok(())
}

/// A start whose agent did not come up, whatever the runtime: forgets the session (or, for
/// an adopted conversation, gives it back to its project as `restore` was), releases the
/// workspace and puts the task where [`abort_start`] does, so it can be started again (the
/// app offers the terminal for a chat-mode one).
async fn rollback_start(
    app: &AppState,
    key: &TaskKey,
    session: &SessionRecord,
    restore: Option<&SessionRecord>,
    existing: Option<Task>,
    reason: &str,
) {
    if let Some(live) = app.sessions.get(&session.id) {
        let _ = live.runtime.kill();
        live.runtime.wait().await;
        app.sessions.remove_if_same(&session.id, &live.runtime);
    }
    let undone = match restore {
        Some(original) => app.store.unadopt_session(original).await,
        None => app.store.forget_session(&session.id).await,
    };
    if let Err(e) = undone {
        tracing::warn!(task = %key, "cannot undo the session: {e}");
    }
    if let Err(e) = crate::archive::release_workspace(app, key, true).await {
        tracing::warn!(task = %key, "cannot release the workspace: {}", e.message());
    }
    if let Err(e) = app.store.delete_workspace(key).await {
        tracing::warn!(task = %key, "cannot forget the workspace: {e}");
    }
    if let Err(e) = abort_start(app, key, existing, true, reason).await {
        tracing::warn!(task = %key, "cannot abort the start: {}", e.message());
    }
    app.emit(crate::events::Topic::Sessions);
}

/// The `saving-mode` preference: off unless the user turned it on.
async fn saving_mode_preference(app: &AppState) -> bool {
    let value: Option<serde_json::Value> =
        app.store.setting("pref:saving-mode").await.ok().flatten();
    value
        .as_ref()
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false)
}

/// A fresh autopilot: running, nothing nudged yet.
fn new_autopilot() -> delilah_core::Autopilot {
    delilah_core::Autopilot {
        state: delilah_core::AutopilotState::Running,
        stage: None,
        reason: None,
        since: chrono::Utc::now(),
        nudged: None,
        review_session: None,
    }
}

/// A task's first (main) session and the prompt it starts with. In saving mode it is
/// the plan step, told to stop once the tasks are written; with autopilot it is also
/// told to answer the workflow's questions itself.
fn first_session(
    app: &AppState,
    task: &str,
    saving_mode: bool,
    autopilot: bool,
    prompt: String,
) -> (SessionRecord, String) {
    let lang = app.lang();
    let (label, step, prompt) = match saving_mode {
        true => (
            crate::handoff::step_label(lang, Step::Plan).to_string(),
            Some(Step::Plan),
            match autopilot {
                true => format!(
                    "{prompt} {}",
                    crate::autopilot::autopilot_plan_note(lang, task)
                ),
                false => format!("{prompt} {}", crate::handoff::plan_note(lang)),
            },
        ),
        false => (lang.pick("Principal", "Main").to_string(), None, prompt),
    };
    let session = SessionRecord {
        id: uuid::Uuid::new_v4().to_string(),
        task: task.to_string(),
        kind: SessionKind::Claude,
        label,
        primary: true,
        closed: false,
        read_only: false,
        conversation: false,
        step,
    };
    (session, prompt)
}

/// How long a chat-mode agent has, after its first message, to show any sign of work (its
/// `Init` line, a message, a result) before the start fails. The real CLI prints nothing
/// to stdout but hook lines until it has read that message.
pub(crate) const FIRST_TURN_TIMEOUT: Duration = Duration::from_secs(60);

/// How long a freshly spawned chat-mode agent must stay alive before it counts as up when
/// there is nothing to wait for (no first message to send): an agent that dies on its
/// flags or its login does so well within it.
pub(crate) const SETTLE: Duration = Duration::from_millis(700);

/// Whether a session runs on a stream (chat mode) rather than a terminal: only a
/// task's Claude sessions do, when the task has the flag. Project sessions and idea
/// conversations are always terminals.
pub(crate) async fn runtime_kind(
    app: &AppState,
    session: &SessionRecord,
) -> Result<RuntimeKind, ApiError> {
    if session.kind != SessionKind::Claude
        || session.conversation
        || project_of(&session.task).is_some()
    {
        return Ok(RuntimeKind::Pty);
    }
    // The session's own runtime wins over the task's flag.
    let chat = match app
        .store
        .session_extras(&session.id)
        .await?
        .runtime_override
    {
        Some(kind) => kind == RuntimeKind::Stream,
        None => app
            .store
            .get_task(&TaskKey::new(&session.task))
            .await?
            .is_some_and(|t| t.chat_mode),
    };
    Ok(match chat {
        true => RuntimeKind::Stream,
        false => RuntimeKind::Pty,
    })
}

/// Starts a session in a daemon-owned PTY (or, for a chat-mode task, a stream-json
/// process): Claude (new, or resumed with `--resume` on the same id) or the user's
/// login shell, in the task's workspace. A stream session gets the prompt on stdin once
/// it has stayed alive for a moment, and is up when it shows signs of work on it (or,
/// with no prompt, when it is still alive). Boxed, so the handoffs can take it (and
/// tests replace it) as a [`crate::handoff::Launcher`].
pub fn launch_session<'a>(
    app: &'a AppState,
    session: &'a SessionRecord,
    workspace: &'a std::path::Path,
    prompt: Option<&'a str>,
    resume: bool,
) -> crate::handoff::LaunchFuture<'a> {
    Box::pin(launch_session_with(
        app,
        session,
        workspace,
        prompt,
        resume,
        FIRST_TURN_TIMEOUT,
    ))
}

/// [`launch_session`] with its own settle: how long a stream agent started without a
/// prompt must stay alive to count as up.
pub(crate) async fn launch_session_settled(
    app: &AppState,
    session: &SessionRecord,
    workspace: &std::path::Path,
    resume: bool,
    settle: Duration,
) -> Result<(), ApiError> {
    spawn_session(
        app,
        session,
        workspace,
        None,
        resume,
        FIRST_TURN_TIMEOUT,
        settle,
        session_guard(session),
    )
    .await?
    .send_first(app)
    .await
}

/// Whether a session starts with the branch guard hook. Not told whether the task has its
/// branch: any free task gets the guard, which answers at once once the branch exists.
fn session_guard(session: &SessionRecord) -> bool {
    crate::branching::is_free(&TaskKey::new(&session.task))
}

pub(crate) async fn launch_session_with(
    app: &AppState,
    session: &SessionRecord,
    workspace: &std::path::Path,
    prompt: Option<&str>,
    resume: bool,
    first_turn_timeout: Duration,
) -> Result<(), ApiError> {
    spawn_session(
        app,
        session,
        workspace,
        prompt,
        resume,
        first_turn_timeout,
        SETTLE,
        session_guard(session),
    )
    .await?
    .send_first(app)
    .await
}

/// A spawned session that has not yet been given its first message (a stream one).
pub(crate) struct Spawned {
    id: String,
    runtime: Runtime,
    /// A chat-mode agent's first message and how it is watched; nothing for a terminal
    /// (it has its prompt on the command line) or a resume without a prompt.
    first: Option<(String, StreamStart)>,
    timeout: Duration,
}

/// Whether the process shows signs of work: the pump's word on it.
struct StreamStart {
    started: tokio::sync::oneshot::Receiver<crate::stream_events::Started>,
    up: bool,
}

enum Wait {
    Up,
    Exited,
    /// Its first word was an error (what it said).
    Failed(String),
    TimedOut,
}

impl StreamStart {
    async fn wait(&mut self, limit: Duration) -> Wait {
        if self.up {
            return Wait::Up;
        }
        match tokio::time::timeout(limit, &mut self.started).await {
            Ok(Ok(crate::stream_events::Started::Up)) => {
                self.up = true;
                Wait::Up
            }
            Ok(Ok(crate::stream_events::Started::Failed(said))) => Wait::Failed(said),
            // Exited, or the pump ended without a word (the process is gone).
            Ok(_) => Wait::Exited,
            Err(_) => Wait::TimedOut,
        }
    }
}

impl Spawned {
    /// Stops the process and forgets the live session.
    pub(crate) async fn discard(&self, app: &AppState) {
        let _ = self.runtime.kill();
        self.runtime.wait().await;
        app.sessions.remove_if_same(&self.id, &self.runtime);
    }

    /// Gives a chat-mode agent its first message and waits for it to show signs of work:
    /// the real CLI says nothing (not even its `system/init`) before it. An agent that
    /// exits or stays silent fails the start; any failure leaves no session behind.
    pub(crate) async fn send_first(mut self, app: &AppState) -> Result<(), ApiError> {
        let Some((prompt, mut start)) = self.first.take() else {
            return Ok(());
        };
        let sent = self
            .runtime
            .send_prompt(&prompt)
            .await
            .map_err(|e| ApiError::internal(format!("cannot send the first message: {e}")));
        let failure = match sent {
            Err(e) => Some(e),
            Ok(()) => match start.wait(self.timeout).await {
                Wait::Up => None,
                Wait::Failed(said) => Some(start_failure(app, &self.runtime, Some(&said)).await),
                Wait::Exited | Wait::TimedOut => {
                    Some(start_failure(app, &self.runtime, None).await)
                }
            },
        };
        match failure {
            Some(e) => {
                self.discard(app).await;
                Err(e)
            }
            None => Ok(()),
        }
    }
}

/// The error of a chat-mode agent that did not come up: what it said as its error result,
/// or else on stderr.
async fn start_failure(app: &AppState, runtime: &Runtime, said: Option<&str>) -> ApiError {
    let _ = runtime.kill();
    runtime.wait().await;
    let tail = match said.filter(|s| !s.trim().is_empty()) {
        Some(said) => start_error_tail(said),
        None => runtime
            .as_stream()
            .map(|s| start_error_tail(&s.stderr_tail()))
            .unwrap_or_default(),
    };
    let tail = match tail.trim() {
        "" => "claude printed nothing before it stopped",
        tail => tail,
    };
    let tail = with_permission_hint(app, tail);
    ApiError::unprocessable(format!("chat mode could not start: {tail}"))
}

/// Whether `text` is the CLI refusing the `--permission-mode` it was launched with (an
/// unknown or unavailable mode). Keyed on the rejection's shape, not on the mode named:
/// `error: option '--permission-mode <mode>' argument '<x>' is invalid. Allowed choices
/// are ...` (docs/archive/spikes/2026-10-26-chat-permission-mode.md).
fn rejects_permission_mode(text: &str) -> bool {
    let text = text.to_lowercase();
    text.contains("--permission-mode")
        && ["invalid", "not available", "unavailable"]
            .iter()
            .any(|w| text.contains(w))
}

/// `text` with the hint that names the setting that changes the mode, when the CLI
/// rejected it (FR-008); any other text as it is.
fn with_permission_hint(app: &AppState, text: &str) -> String {
    if !rejects_permission_mode(text) {
        return text.to_string();
    }
    let hint = app.lang().pick(
        "Ajustes → Permisos de los agentes: prueba \"Por defecto de Claude\"",
        "Settings → Agent permissions: try \"Claude's default\"",
    );
    format!("{text}\n{hint}")
}

/// How soon after its spawn a terminal agent must exit for its exit to be the CLI
/// rejecting its flags: that happens before the CLI draws anything (spike, FR-008).
const MODE_REJECTION_WINDOW: Duration = Duration::from_secs(10);

/// How many of a screen's first non-empty lines can hold the rejection: the CLI prints
/// it before anything else, so a line further down is the agent's own output (a diff,
/// a test run) quoting it.
const MODE_REJECTION_HEAD: usize = 5;

/// The lines at the top of a terminal screen where the CLI rejects the permission mode
/// it was launched with, joined; empty when there are none.
fn rejection_lines(screen: &str) -> String {
    crate::setup::strip_escapes(screen)
        .replace('\r', "\n")
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .take(MODE_REJECTION_HEAD)
        .filter(|l| rejects_permission_mode(l))
        .collect::<Vec<_>>()
        .join("\n")
}

/// A terminal agent that exited, right after its spawn, with the CLI's rejection of the
/// permission mode at the top of its screen (FR-008): an exit before any work. A later
/// exit (a kill on Detener, a runtime switch, a crash) is never this, whatever the
/// screen says. A task fails with that line and the hint; a project's session has no
/// task to fail, so it raises an error in the inbox, as a failed turn does.
async fn fail_if_mode_rejected(
    app: &AppState,
    task: &str,
    session: &str,
    pty: &PtySession,
    lived: Duration,
) {
    if lived > MODE_REJECTION_WINDOW || pty.exit_code().is_none_or(|c| c == 0) {
        return;
    }
    let (screen, _) = pty.attach();
    let said = rejection_lines(&String::from_utf8_lossy(&screen));
    if said.is_empty() {
        return;
    }
    if project_of(task).is_some() {
        let attention = crate::inbox::Attention {
            kind: crate::inbox::AttentionKind::Error,
            text: with_permission_hint(app, &start_error_tail(&said)),
            since: chrono::Utc::now(),
        };
        match app.store.set_attention(session, Some(&attention)).await {
            Ok(()) => app.emit(Topic::Inbox),
            Err(e) => tracing::warn!(session, "cannot record the rejected mode: {e}"),
        }
        return;
    }
    let key = TaskKey::new(task);
    let reason = format!(
        "the agent could not start: {}",
        with_permission_hint(app, &start_error_tail(&said))
    );
    if let Err(e) = app.transition(&key, Event::Fail { reason }).await {
        tracing::warn!(task = %key, "cannot fail the task: {e}");
    }
}

/// Starts the session's process and registers it, but a chat-mode agent is not given its
/// first message yet: that is [`Spawned::send_first`]. For a stream session, fails if the
/// process does not stay alive for a moment.
/// `settle`: how long a stream agent with no first message must stay alive to count as up.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn spawn_session(
    app: &AppState,
    session: &SessionRecord,
    workspace: &std::path::Path,
    prompt: Option<&str>,
    resume: bool,
    first_turn_timeout: Duration,
    settle: Duration,
    guard: bool,
) -> Result<Spawned, ApiError> {
    // Nothing to resume if Claude never got a first message: start it afresh.
    let resume =
        resume && claude::has_conversation_in(&app.config.claude_projects, workspace, &session.id);
    // No control character reaches the agent's command line.
    let prompt = prompt.map(crate::actions::sanitize_prompt);
    let prompt = prompt.as_deref();
    let kind = runtime_kind(app, session).await?;
    let task_dir = app.config.task_dir(&session.task);
    std::fs::create_dir_all(&task_dir)
        .map_err(|e| ApiError::internal(format!("cannot create {}: {e}", task_dir.display())))?;
    let log = task_dir.join(format!("{}-{}.log", session.kind.name(), session.id));
    let spawn_error =
        |e: std::io::Error| ApiError::internal(format!("cannot start the session: {e}"));
    let (runtime, events) = match session.kind {
        SessionKind::Claude => {
            let (permission, plugin_dir, append_system_prompt) =
                launch_flags(session, &app.config, app.agent_permissions());
            // The rules "always allow" added to the task, read now so a new session has them.
            let allowed_tools = match project_of(&session.task) {
                Some(_) => Vec::new(),
                None => app
                    .store
                    .get_task(&TaskKey::new(&session.task))
                    .await?
                    .map(|t| t.allowed_tools)
                    .unwrap_or_default(),
            };
            let settings = claude::write_settings(
                &task_dir.join(format!("claude-{}.json", session.id)),
                &app.config.url(),
                &session.task,
                &session.id,
                permission,
                &[task_dir.as_path()],
                &allowed_tools,
                guard,
            )
            .map_err(|e| LaunchError::Start(format!("cannot write agent settings: {e}")))?;
            let remote_name = app.remote_control().then(|| remote_name(session));
            let launch = claude::Launch {
                workspace,
                session_id: &session.id,
                settings: &settings,
                // A stream agent gets its first prompt on stdin, after it is up.
                prompt: prompt.filter(|_| kind == RuntimeKind::Pty),
                resume,
                extra_dirs: &[task_dir.as_path()],
                permission,
                plugin_dir: plugin_dir.as_deref(),
                append_system_prompt,
                remote_control: remote_name.as_deref(),
                program: app.config.agent_program.as_deref(),
            };
            runtime::spawn_with_events(&launch, kind, Some(log)).map_err(spawn_error)?
        }
        SessionKind::Shell => (
            runtime::spawn_pty(shell_command(workspace), Some(log)).map_err(spawn_error)?,
            None,
        ),
        SessionKind::Setup => {
            return Err(ApiError::conflict(
                "a setup session is started by the task and cannot be reopened",
            ));
        }
    };
    let live = app.sessions.insert(
        &session.id,
        &session.task,
        session.kind,
        session.primary,
        runtime.clone(),
    );
    live.set_idea(session.conversation);
    let remote_control =
        session.kind == SessionKind::Claude && kind == RuntimeKind::Pty && app.remote_control();
    tracing::info!(task = %session.task, session = %session.id, kind = session.kind.name(), resume, remote_control, chat = kind == RuntimeKind::Stream, "session started");

    // Only terminals: a stream session skips the trust dialog, as print mode does not
    // wait on it (spike d1, decision 67).
    if session.kind == SessionKind::Claude
        && let Some(pty) = runtime.as_pty()
    {
        tokio::spawn(accept_trust_dialog(
            Arc::clone(pty),
            Arc::clone(&live.started),
        ));
    }
    let sessions = Arc::clone(&app.sessions);
    let id = session.id.clone();
    let watched = runtime.clone();
    let exit_app = app.clone();
    let exiting = live.clone();
    let rejection_watch = (session.kind == SessionKind::Claude && kind == RuntimeKind::Pty)
        .then(|| (app.clone(), session.task.clone(), session.id.clone()));
    let spawned_at = std::time::Instant::now();
    tokio::spawn(async move {
        watched.wait().await;
        // A Stop held behind background work applies first, as after a normal turn end;
        // this process's own session, even if already out of the registry, never a newer one's.
        crate::stream_events::on_process_exit(&exit_app, &exiting).await;
        sessions.remove_if_same(&id, &watched);
        tracing::info!(session = %id, "session exited");
        if let Some((app, task, session)) = rejection_watch
            && let Some(pty) = watched.as_pty()
        {
            fail_if_mode_rejected(&app, &task, &session, pty, spawned_at.elapsed()).await;
        }
    });

    let mut spawned = Spawned {
        id: session.id.clone(),
        runtime: runtime.clone(),
        first: None,
        timeout: first_turn_timeout,
    };
    if let Some(events) = events {
        let started = crate::stream_events::spawn_pump(app, &session.id, &session.task, events);
        let mut start = StreamStart { started, up: false };
        // An agent that dies on its flags or login does so at once; it says nothing else
        // until it has a message. With no message to send (a resumed conversation), this
        // brief wait is the only check of the start: an agent that dies later is seen
        // only by the exit watcher above, which drops its session from the registry.
        let early = match start.wait(first_turn_timeout.min(settle)).await {
            Wait::Exited => Some(None),
            Wait::Failed(said) => Some(Some(said)),
            Wait::Up | Wait::TimedOut => None,
        };
        if let Some(said) = early {
            let e = start_failure(app, &runtime, said.as_deref()).await;
            spawned.discard(app).await;
            return Err(e);
        }
        spawned.first = prompt.map(|p| (p.to_string(), start));
    }
    Ok(spawned)
}

/// How a session is listed in claude.ai under Remote Control: the task (or project)
/// and the session's label, as Delilah shows them. The label is fixed when the
/// session is created, so the name stays the same if the language changes later.
fn remote_name(session: &SessionRecord) -> String {
    let owner = project_of(&session.task).unwrap_or(&session.task);
    format!("{owner} · {}", session.label)
}

/// The user's login shell, so their PATH and aliases are there.
fn shell_command(workspace: &std::path::Path) -> portable_pty::CommandBuilder {
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".into());
    let mut cmd = portable_pty::CommandBuilder::new(shell);
    cmd.arg("-l");
    cmd.env("TERM", "xterm-256color");
    cmd.cwd(workspace);
    cmd
}

/// A new workspace path shows Claude's folder-trust dialog once: answer "Yes". The
/// dialog may draw itself again (back on "No") right after appearing, so let it
/// settle, move to "Yes", and press Enter only once "Yes" is seen selected. Stops as
/// soon as the session starts, so it never types into the user's input.
async fn accept_trust_dialog(pty: Arc<PtySession>, started: Arc<tokio::sync::Notify>) {
    let (mut screen, mut rx) = pty.attach();
    let deadline = tokio::time::Instant::now() + TRUST_WATCH;
    loop {
        let settle = match claude::trust_dialog_selection(&screen) {
            None => Duration::from_millis(200),
            // A key pressed while it still redraws would be undone.
            Some(_) => Duration::from_millis(600),
        };
        let before = claude::trust_dialog_selection(&screen);
        if !read_for(&mut rx, &mut screen, &started, settle, deadline).await {
            return;
        }
        let Some(yes) = claude::trust_dialog_selection(&screen) else {
            continue;
        };
        if before != Some(yes) {
            // Still changing: look again.
            continue;
        }
        if yes {
            let _ = pty.write(claude::ENTER);
            tracing::info!("accepted the folder-trust dialog");
            return;
        }
        let _ = pty.write(claude::DOWN);
    }
}

/// Collects terminal output into `screen` for `wait`. False once the session started,
/// the terminal closed or `deadline` passed.
async fn read_for(
    rx: &mut tokio::sync::broadcast::Receiver<Arc<[u8]>>,
    screen: &mut Vec<u8>,
    started: &tokio::sync::Notify,
    wait: Duration,
    deadline: tokio::time::Instant,
) -> bool {
    let until = (tokio::time::Instant::now() + wait).min(deadline);
    loop {
        tokio::select! {
            chunk = rx.recv() => match chunk {
                Ok(chunk) => screen.extend_from_slice(&chunk),
                Err(RecvError::Lagged(_)) => continue,
                Err(RecvError::Closed) => return false,
            },
            _ = started.notified() => return false,
            _ = tokio::time::sleep_until(until) => return tokio::time::Instant::now() < deadline,
        }
    }
}

/// The user's notes, like the ticket: a file in the task's folder (outside the repo)
/// and a pointer to it in the first prompt, whatever the prompt is.
fn point_to_notes(app: &AppState, key: &TaskKey, prompt: String, notes: Option<&str>) -> String {
    let Some(notes) = notes.filter(|n| !n.trim().is_empty()) else {
        return prompt;
    };
    let dir = app.config.task_dir(key.as_str());
    let path = dir.join("notes.md");
    match std::fs::create_dir_all(&dir).and_then(|()| std::fs::write(&path, notes)) {
        Ok(()) => format!(
            "{prompt} ({} {})",
            app.lang().pick(
                "tus notas sobre la tarea están en",
                "your notes on the task are at"
            ),
            path.display()
        ),
        Err(e) => {
            tracing::warn!(task = %key, "cannot write the notes: {e}");
            prompt
        }
    }
}

/// The setting that holds the last free task's number handed out.
const FREE_KEY_LAST: &str = "free_key_last";

/// The key the next free task will get, without handing it out.
pub(crate) async fn peek_free_key(app: &AppState) -> Result<String, ApiError> {
    let existing = app
        .store
        .list_tasks()
        .await?
        .iter()
        .filter_map(|t| t.key.as_str().strip_prefix("LIBRE-")?.parse::<u32>().ok())
        .max()
        .unwrap_or(0);
    let last: u32 = app.store.setting(FREE_KEY_LAST).await?.unwrap_or(0);
    Ok(format!("LIBRE-{}", last.max(existing) + 1))
}

/// The next free task's key: `LIBRE-1`, `LIBRE-2`… A number is never handed out
/// twice, not even after its task is deleted (its `feature/LIBRE-n` branch may live
/// on) or to two tasks created at once.
pub(crate) async fn next_free_key(app: &AppState) -> Result<String, ApiError> {
    // Tasks from before the setting existed count too.
    let existing = app
        .store
        .list_tasks()
        .await?
        .iter()
        .filter_map(|t| t.key.as_str().strip_prefix("LIBRE-")?.parse::<u32>().ok())
        .max()
        .unwrap_or(0);
    let n = app.store.next_counter(FREE_KEY_LAST, existing).await?;
    Ok(format!("LIBRE-{n}"))
}

/// The submodules a parent task gets: the request's, then the `always` ones not yet in it.
/// Refuses a rule naming a submodule the parent no longer has, and a request naming a
/// `never` one.
fn effective_submodules(
    requested: &[String],
    rules: &delilah_core::SubmoduleRules,
    available: &[String],
    lang: crate::lang::Lang,
) -> Result<Vec<String>, Vec<delilah_core::FieldError>> {
    let mut errors = Vec::new();
    for (list, key) in [(&rules.always, "always"), (&rules.never, "never")] {
        for (i, path) in list.iter().enumerate() {
            if !available.contains(path) {
                errors.push(delilah_core::FieldError {
                    field: format!("settings.submodules.{key}[{i}]"),
                    code: "submodules.stale_rule".into(),
                    message: match lang {
                        crate::lang::Lang::Es => format!(
                            "`{path}` ya no existe en el proyecto; quítalo en los ajustes del proyecto."
                        ),
                        crate::lang::Lang::En => format!(
                            "`{path}` no longer exists in the project; remove it in the project's settings."
                        ),
                    },
                });
            }
        }
    }
    for (i, path) in requested.iter().enumerate() {
        if rules.never.contains(path) {
            errors.push(delilah_core::FieldError {
                field: format!("submodules[{i}]"),
                code: "submodules.never".into(),
                message: match lang {
                    crate::lang::Lang::Es => {
                        format!("`{path}` está excluido en los ajustes del proyecto (never).")
                    }
                    crate::lang::Lang::En => {
                        format!("`{path}` is excluded in the project's settings (never).")
                    }
                },
            });
        }
    }
    if !errors.is_empty() {
        return Err(errors);
    }
    let mut merged: Vec<String> = Vec::new();
    for path in requested.iter().chain(&rules.always) {
        if !merged.contains(path) {
            merged.push(path.clone());
        }
    }
    Ok(merged)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::AgentPermissions::Auto;

    #[test]
    fn every_selected_submodule_is_on_the_task_branch() {
        let dir = std::env::temp_dir().join(format!("delilah-run-ws-{}", uuid::Uuid::new_v4()));
        let fixture = std::process::Command::new("bash")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../scripts/make-fixture-parent.sh"
            ))
            .arg(dir.join("env"))
            .output()
            .unwrap();
        assert!(fixture.status.success());
        let project = delilah_core::Project {
            name: "parent".into(),
            path: dir.join("env/parent"),
            kind: ProjectKind::Parent,
            workflow: "feature".into(),
            settings: Default::default(),
            preview_command: None,
        };
        let subs = vec!["svc-orders".to_string(), "svc-payments".to_string()];
        let spec = WorkspaceSpec {
            path: dir.join("workspaces/PROJ-9"),
            branch: Some("feature/PROJ-9-add-retry".into()),
            kind: project.kind,
            submodules: subs.clone(),
        };
        let opts = GitOptions {
            allow_file_protocol: true,
        };
        let (ws, _) = delilah_git::create_workspace(
            &project.path,
            &spec,
            &opts,
            Deadline::after(Duration::from_secs(30)),
        )
        .unwrap();
        for sub in &subs {
            let out = std::process::Command::new("git")
                .arg("-C")
                .arg(ws.path.join(sub))
                .args(["branch", "--show-current"])
                .output()
                .unwrap();
            assert_eq!(
                String::from_utf8_lossy(&out.stdout).trim(),
                "feature/PROJ-9-add-retry",
                "{sub}"
            );
        }
        std::fs::remove_dir_all(dir).unwrap();
    }

    fn session(task: &str, label: &str) -> SessionRecord {
        SessionRecord {
            id: "sess-1".into(),
            task: task.into(),
            kind: SessionKind::Claude,
            label: label.into(),
            primary: true,
            closed: false,
            read_only: false,
            conversation: false,
            step: None,
        }
    }

    #[test]
    fn the_launch_table() {
        let config = crate::config::Config {
            home: "/h".into(),
            addr: crate::config::DEFAULT_ADDR.parse().unwrap(),
            allow_file_protocol: false,
            notify: false,
            ui_origin: None,
            agent_program: None,
            ..crate::config::Config::test_defaults()
        };
        let mut conv = session("@web", "Idea");
        conv.conversation = true;
        let (p, dir, prompt) = launch_flags(&conv, &config, Auto);
        assert_eq!(p, claude::Permission::DontAsk);
        assert_eq!(dir, Some(config.plugin_dir()));
        assert_eq!(prompt, Some(IDEA_PROMPT));

        let mut ro = session("@web", "Claude 1");
        ro.read_only = true;
        let (p, dir, prompt) = launch_flags(&ro, &config, Auto);
        assert_eq!(p, claude::Permission::Plan);
        assert!(dir.is_some());
        assert_eq!(prompt, None);

        let (p, dir, prompt) = launch_flags(&session("PROJ-1", "Claude 1"), &config, Auto);
        assert_eq!(p, claude::Permission::Auto);
        assert_eq!((dir, prompt), (None, None));

        // An extra session of a task is no different: it never gets the plugin.
        let mut extra = session("PROJ-1", "Claude 2");
        extra.primary = false;
        let (p, dir, prompt) = launch_flags(&extra, &config, Auto);
        assert_eq!(p, claude::Permission::Auto);
        assert_eq!((dir, prompt), (None, None));
        extra.read_only = true;
        let (p, dir, prompt) = launch_flags(&extra, &config, Auto);
        assert_eq!(p, claude::Permission::Plan);
        assert_eq!((dir, prompt), (None, None));
    }

    const SPIKE_REJECTION: &str = "error: option '--permission-mode <mode>' argument 'bogus' is invalid. Allowed choices are acceptEdits, auto, bypassPermissions, manual, dontAsk, plan.";

    #[test]
    fn the_cli_rejecting_the_permission_mode_is_recognised_and_nothing_else_is() {
        assert!(rejects_permission_mode(SPIKE_REJECTION));
        assert!(rejects_permission_mode(
            "--permission-mode auto is not available for this account"
        ));
        assert!(!rejects_permission_mode("Invalid API key"));
        assert!(!rejects_permission_mode("no such flag"));
        assert!(!rejects_permission_mode("--permission-mode auto"));
    }

    #[test]
    fn the_rejection_is_picked_out_of_the_top_of_a_screen() {
        let screen = format!(
            "\x1b[31mWelcome to Claude\x1b[0m\r\n\r\n  \x1b[1m{SPIKE_REJECTION}\x1b[0m  \r\nInvalid API key\r\nTry again\r\n{SPIKE_REJECTION}\r\n"
        );
        assert_eq!(
            rejection_lines(&screen),
            format!("{SPIKE_REJECTION}\n{SPIKE_REJECTION}")
        );
        assert_eq!(rejection_lines("Welcome\r\nInvalid API key\r\n"), "");
    }

    #[test]
    fn a_rejection_quoted_further_down_the_screen_is_not_one() {
        let work = (1..=MODE_REJECTION_HEAD)
            .map(|n| format!("● step {n}\r\n"))
            .collect::<String>();
        assert_eq!(
            rejection_lines(&format!("{work}+ const R = \"{SPIKE_REJECTION}\";\r\n")),
            ""
        );
    }

    #[tokio::test]
    async fn an_exit_long_after_the_spawn_is_never_a_rejected_mode() {
        let app = app().await;
        let mut cmd = portable_pty::CommandBuilder::new("sh");
        cmd.args(["-c", &format!("echo \"{SPIKE_REJECTION}\"; exit 1")]);
        let pty = PtySession::spawn(cmd, 200, 24, None).unwrap();
        pty.wait().await;
        let owner = crate::store_sessions::project_owner("p");
        let mut rx = app.events.subscribe();
        let late = MODE_REJECTION_WINDOW + Duration::from_secs(1);
        fail_if_mode_rejected(&app, &owner, "s-1", &pty, late).await;
        assert!(rx.try_recv().is_err(), "a late exit raised an inbox error");
        fail_if_mode_rejected(&app, &owner, "s-1", &pty, Duration::ZERO).await;
        assert_eq!(rx.try_recv().ok(), Some(Topic::Inbox));
    }

    #[tokio::test]
    async fn a_rejected_permission_mode_gets_the_settings_hint_in_each_language() {
        let app = app().await;
        app.lang.set(crate::lang::Lang::En);
        let said = with_permission_hint(&app, SPIKE_REJECTION);
        assert!(said.starts_with(SPIKE_REJECTION), "{said}");
        assert!(
            said.ends_with("Settings → Agent permissions: try \"Claude's default\""),
            "{said}"
        );
        app.lang.set(crate::lang::Lang::Es);
        assert!(
            with_permission_hint(&app, SPIKE_REJECTION)
                .ends_with("Ajustes → Permisos de los agentes: prueba \"Por defecto de Claude\""),
        );
        assert_eq!(
            with_permission_hint(&app, "Invalid API key"),
            "Invalid API key"
        );
    }

    #[test]
    fn the_agent_permissions_preference_only_changes_task_and_project_sessions() {
        use crate::state::AgentPermissions::Default as Asks;
        let config = crate::config::Config::test_defaults();
        for key in ["PROJ-1", "@web"] {
            let (p, ..) = launch_flags(&session(key, "Claude 1"), &config, Auto);
            assert_eq!(p, claude::Permission::Auto, "{key}");
            let (p, ..) = launch_flags(&session(key, "Claude 1"), &config, Asks);
            assert_eq!(p, claude::Permission::Default, "{key}");
        }
        let mut conv = session("@web", "Idea");
        conv.conversation = true;
        let mut ro = session("@web", "Claude 1");
        ro.read_only = true;
        for mode in [Auto, Asks] {
            assert_eq!(
                launch_flags(&conv, &config, mode).0,
                claude::Permission::DontAsk
            );
            assert_eq!(launch_flags(&ro, &config, mode).0, claude::Permission::Plan);
        }
    }

    async fn app() -> AppState {
        let config = crate::config::Config {
            home: std::env::temp_dir().join(format!("delilah-run-{}", uuid::Uuid::new_v4())),
            addr: crate::config::DEFAULT_ADDR.parse().unwrap(),
            allow_file_protocol: false,
            notify: false,
            ui_origin: None,
            claude_bin: "/usr/bin/false".into(),
            agent_program: None,
            ..crate::config::Config::test_defaults()
        };
        let app = AppState::new(crate::store::Store::in_memory().await.unwrap(), config);
        app.lang.set(crate::lang::Lang::Es); // Spanish fixtures
        // A folder that isn't there: any workspace for it fails, and no agent starts.
        app.store
            .create_project(&delilah_core::Project {
                name: "web".into(),
                path: app.config.home.join("missing"),
                kind: ProjectKind::Repo,
                workflow: "feature".into(),
                settings: Default::default(),
                preview_command: None,
            })
            .await
            .unwrap();
        app
    }

    fn run(key: &str) -> RunRequest {
        serde_json::from_value(serde_json::json!({ "key": key, "title": "", "project": "web" }))
            .unwrap()
    }

    #[tokio::test]
    async fn a_worktree_task_on_a_repo_project_starts_at_the_fresh_remote_head() {
        let app = app().await;
        let home = app.config.home.clone();
        let sh = |dir: &std::path::Path, args: &[&str]| {
            let out = std::process::Command::new("git")
                .args(["-c", "user.name=T", "-c", "user.email=t@d.invalid"])
                .arg("-C")
                .arg(dir)
                .args(args)
                .output()
                .unwrap();
            assert!(out.status.success(), "{args:?}");
            String::from_utf8_lossy(&out.stdout).trim().to_string()
        };
        let origin = home.join("origin.git");
        let repo = home.join("repo");
        let other = home.join("other");
        std::fs::create_dir_all(&home).unwrap();
        sh(
            &home,
            &[
                "init",
                "-q",
                "--bare",
                "-b",
                "main",
                origin.to_str().unwrap(),
            ],
        );
        sh(
            &home,
            &[
                "clone",
                "-q",
                origin.to_str().unwrap(),
                repo.to_str().unwrap(),
            ],
        );
        std::fs::write(repo.join("a.txt"), "a\n").unwrap();
        sh(&repo, &["add", "."]);
        sh(&repo, &["commit", "-qm", "init"]);
        sh(&repo, &["push", "-q", "origin", "HEAD:main"]);
        // The remote moves on after the clone.
        sh(
            &home,
            &[
                "clone",
                "-q",
                origin.to_str().unwrap(),
                other.to_str().unwrap(),
            ],
        );
        std::fs::write(other.join("b.txt"), "b\n").unwrap();
        sh(&other, &["add", "."]);
        sh(&other, &["commit", "-qm", "ahead"]);
        sh(&other, &["push", "-q", "origin", "HEAD:main"]);
        let remote_head = sh(&other, &["rev-parse", "HEAD"]);
        let folder_head = sh(&repo, &["rev-parse", "HEAD"]);
        let folder_main = sh(&repo, &["rev-parse", "refs/heads/main"]);

        let agent = home.join("agent.sh");
        std::fs::write(&agent, "#!/bin/sh\nexec sleep 5\n").unwrap();
        std::fs::set_permissions(&agent, std::os::unix::fs::PermissionsExt::from_mode(0o755))
            .unwrap();
        let mut app = app;
        app.config = Arc::new(crate::config::Config {
            agent_program: Some(agent),
            ..(*app.config).clone()
        });
        app.store
            .create_project(&delilah_core::Project {
                name: "fresh".into(),
                path: repo.clone(),
                kind: ProjectKind::Repo,
                workflow: "feature".into(),
                settings: Default::default(),
                preview_command: None,
            })
            .await
            .unwrap();

        let mut req = run("PROJ-9");
        req.project = "fresh".into();
        req.title = "Fresh".into();
        req.in_checkout = Some(false);
        let started = start_run(State(app.clone()), Json(req)).await;
        if let Err(e) = &started {
            panic!("start failed: {} {}", e.status, e.message);
        }

        let ws = app.config.workspaces_dir().join("PROJ-9");
        assert_eq!(sh(&ws, &["rev-parse", "HEAD"]), remote_head);
        // The project folder is untouched: same commit, same branch, `main` not moved.
        assert_eq!(sh(&repo, &["rev-parse", "HEAD"]), folder_head);
        assert_eq!(sh(&repo, &["branch", "--show-current"]), "main");
        assert_eq!(sh(&repo, &["rev-parse", "refs/heads/main"]), folder_main);
        assert_ne!(folder_main, remote_head);
    }

    /// A one-commit repo project "stale" in the app's home, with `origin` set to
    /// `origin` when given; the agent is a fake that just sleeps.
    async fn repo_project(origin: Option<&str>) -> AppState {
        let mut app = app().await;
        let home = app.config.home.clone();
        let repo = home.join("stale-repo");
        std::fs::create_dir_all(&repo).unwrap();
        let sh = |args: &[&str]| {
            let out = std::process::Command::new("git")
                .args(["-c", "user.name=T", "-c", "user.email=t@d.invalid"])
                .arg("-C")
                .arg(&repo)
                .args(args)
                .output()
                .unwrap();
            assert!(out.status.success(), "{args:?}");
        };
        sh(&["init", "-q", "-b", "main"]);
        std::fs::write(repo.join("a.txt"), "a\n").unwrap();
        sh(&["add", "."]);
        sh(&["commit", "-qm", "init"]);
        if let Some(origin) = origin {
            sh(&["remote", "add", "origin", origin]);
            // What a past fetch left behind.
            sh(&["update-ref", "refs/remotes/origin/main", "HEAD"]);
        }
        let agent = home.join("agent.sh");
        std::fs::write(&agent, "#!/bin/sh\nexec sleep 5\n").unwrap();
        std::fs::set_permissions(&agent, std::os::unix::fs::PermissionsExt::from_mode(0o755))
            .unwrap();
        app.config = Arc::new(crate::config::Config {
            agent_program: Some(agent),
            ..(*app.config).clone()
        });
        app.store
            .create_project(&delilah_core::Project {
                name: "stale".into(),
                path: repo,
                kind: ProjectKind::Repo,
                workflow: "feature".into(),
                settings: Default::default(),
                preview_command: None,
            })
            .await
            .unwrap();
        app
    }

    async fn start_stale(app: &AppState, key: &str, in_checkout: bool) {
        let mut req = run(key);
        req.project = "stale".into();
        req.title = "Stale".into();
        req.in_checkout = Some(in_checkout);
        if let Err(e) = start_run(State(app.clone()), Json(req)).await {
            panic!("start failed: {} {}", e.status, e.message);
        }
    }

    #[tokio::test]
    async fn a_failed_fetch_starts_the_task_and_leaves_a_base_stale_notice() {
        let app = repo_project(Some("/nonexistent")).await;
        start_stale(&app, "PROJ-8", false).await;

        let notices = app.store.open_notices().await.unwrap();
        assert_eq!(notices.len(), 1, "{notices:?}");
        let n = &notices[0];
        assert_eq!(n.task_key, "PROJ-8");
        assert_eq!((n.kind.as_str(), n.repo.as_str()), ("base-stale", "."));
        assert!(n.cause.starts_with("start "), "{}", n.cause);
        assert!(n.text.contains("stale"), "{}", n.text);
        assert!(n.text.contains("origin/main @ "), "{}", n.text);
        assert!(
            n.text.contains("/nonexistent") || n.text.contains("fetch"),
            "{}",
            n.text
        );
    }

    #[tokio::test]
    async fn a_dirty_checkout_is_refused_with_a_reason_the_client_can_read() {
        let app = repo_project(None).await;
        let repo = app.store.project("stale").await.unwrap().path;
        std::fs::write(repo.join("a.txt"), "changed\n").unwrap();
        let mut req = run("PROJ-9");
        req.project = "stale".into();
        req.title = "Dirty".into();
        req.in_checkout = Some(true);
        let Err(e) = start_run(State(app.clone()), Json(req)).await else {
            panic!("a dirty checkout must be refused");
        };
        assert_eq!(e.status, StatusCode::CONFLICT);
        assert_eq!(e.reason.as_deref(), Some("checkout-dirty"));
        assert!(e.message.contains("uncommitted changes"), "{}", e.message);
        assert!(
            app.store
                .get_task(&TaskKey::new("PROJ-9"))
                .await
                .unwrap()
                .is_none(),
            "a refused start leaves no task behind"
        );
    }

    #[test]
    fn short_reason_keeps_the_fatal_line_without_credentials() {
        let stderr = "remote: noise\nfatal: unable to access 'https://bob:s3cret@host.example/r.git/': \x1b[31mtimeout\n\nPlease make sure you have the correct access rights\n";
        assert_eq!(
            short_reason(stderr),
            "fatal: unable to access 'https://host.example/r.git/': [31mtimeout"
        );
        assert_eq!(
            short_reason("ssh://git@host/x and http://h/y"),
            "ssh://host/x and http://h/y"
        );
        assert_eq!(short_reason(&"x".repeat(500)).len(), 200);
        assert_eq!(short_reason(""), "");
    }

    #[tokio::test]
    async fn a_parent_start_warns_only_for_the_repo_whose_fetch_failed() {
        let app = app().await;
        let key = TaskKey::new("PROJ-5");
        app.store
            .create_task(&Task::new(key.clone(), "x", "feature"))
            .await
            .unwrap();
        let base = |repo: &str, fetch| RepoBase {
            repo: repo.into(),
            refname: "origin/main".into(),
            sha: Some("0123456789abcdef".into()),
            fetch,
        };
        let bases = [
            base(".", FetchOutcome::Fetched),
            base("svc-payments", FetchOutcome::Fetched),
            base(
                "svc-billing",
                FetchOutcome::Failed("fatal: no route".into()),
            ),
            base("svc-local", FetchOutcome::NoRemote),
        ];

        let logs = crate::logs::LogBuffer::default();
        let subscriber = tracing_subscriber::fmt()
            .with_writer(logs.clone())
            .with_ansi(false)
            .finish();
        {
            let _guard = tracing::subscriber::set_default(subscriber);
            report_bases(&app, &key, "platform", &bases).await;
        }

        // Every repo's base is logged, warned or not.
        let lines = logs.tail(50);
        for repo in [
            "repo=.",
            "repo=svc-payments",
            "repo=svc-billing",
            "repo=svc-local",
        ] {
            assert!(
                lines
                    .iter()
                    .any(|l| l.contains("task branch base") && l.contains(repo)),
                "{repo} in {lines:?}"
            );
        }

        let notices = app.store.open_notices().await.unwrap();
        assert_eq!(notices.len(), 1, "{notices:?}");
        assert_eq!(notices[0].repo, "svc-billing");
        assert!(
            notices[0].text.contains("svc-billing (fatal: no route)"),
            "{}",
            notices[0].text
        );
        // Dismissed, it comes back at the next start: the cause is the start's moment,
        // not swallowed by the unique index.
        assert!(app.store.resolve_notice(notices[0].id).await.unwrap());
        report_bases(&app, &key, "platform", &bases).await;
        assert_eq!(app.store.open_notices().await.unwrap().len(), 1);
        assert!(
            notices[0].text.contains("origin/main @ 0123456"),
            "{}",
            notices[0].text
        );
    }

    #[tokio::test]
    async fn no_remote_leaves_no_notice_in_either_mode() {
        let app = repo_project(None).await;
        start_stale(&app, "PROJ-7", false).await;
        assert!(app.store.open_notices().await.unwrap().is_empty());

        let app = repo_project(None).await;
        start_stale(&app, "PROJ-6", true).await;
        assert!(app.store.open_notices().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn a_taken_branch_in_checkout_mode_gets_a_fresh_one() {
        // The free-name logic picks `-2`, so the start is re-based (and warns) rather
        // than switching to the existing branch.
        let app = repo_project(Some("/nonexistent")).await;
        let repo = app.config.home.join("stale-repo");
        let out = std::process::Command::new("git")
            .arg("-C")
            .arg(&repo)
            .args(["branch", "feature/PROJ-6"])
            .output()
            .unwrap();
        assert!(out.status.success());
        start_stale(&app, "PROJ-6", true).await;
        let notices = app.store.open_notices().await.unwrap();
        assert_eq!(notices.len(), 1, "{notices:?}");
        assert_eq!(notices[0].kind, "base-stale");
    }

    #[tokio::test]
    async fn a_successful_fetch_leaves_no_notice() {
        let app = repo_project(None).await;
        let home = app.config.home.clone();
        let origin = home.join("good-origin.git");
        let run_git = |args: &[&str]| {
            let out = std::process::Command::new("git")
                .args(args)
                .output()
                .unwrap();
            assert!(out.status.success(), "{args:?}");
        };
        run_git(&[
            "clone",
            "-q",
            "--bare",
            home.join("stale-repo").to_str().unwrap(),
            origin.to_str().unwrap(),
        ]);
        run_git(&[
            "-C",
            home.join("stale-repo").to_str().unwrap(),
            "remote",
            "add",
            "origin",
            origin.to_str().unwrap(),
        ]);
        start_stale(&app, "PROJ-5", false).await;
        assert!(app.store.open_notices().await.unwrap().is_empty());
    }

    async fn backlog_task(app: &AppState, key: &str) -> TaskKey {
        let mut task = Task::new(TaskKey::new(key), "Reintentos", "feature");
        task.project = Some("web".into());
        app.store.create_task(&task).await.unwrap();
        task.key
    }

    fn git(dir: &std::path::Path, args: &[&str]) -> String {
        let out = std::process::Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(["-c", "user.name=t", "-c", "user.email=t@t"])
            .args(args)
            .output()
            .unwrap();
        assert!(out.status.success(), "git {args:?}");
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    }

    /// A daemon with a real repo project `web` and a fake `claude` that runs `script`.
    async fn repo_app(script: &str) -> (AppState, std::path::PathBuf) {
        use std::os::unix::fs::PermissionsExt;
        let mut app = app().await;
        let repo = app.config.home.join("web");
        std::fs::create_dir_all(&repo).unwrap();
        git(&repo, &["init", "-q", "-b", "main"]);
        git(&repo, &["commit", "-q", "--allow-empty", "-m", "init"]);
        let bin = app.config.home.join("claude");
        std::fs::write(&bin, format!("#!/bin/sh\n{script}\n")).unwrap();
        std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
        let mut config = app.config.as_ref().clone();
        config.claude_bin = bin;
        app = AppState::new(app.store.clone(), config);
        app.lang.set(crate::lang::Lang::Es); // Spanish fixtures
        app.store
            .create_project(&delilah_core::Project {
                name: "repo".into(),
                path: repo.clone(),
                kind: ProjectKind::Repo,
                workflow: "feature".into(),
                settings: Default::default(),
                preview_command: None,
            })
            .await
            .unwrap();
        (app, repo)
    }

    /// Starts a task in `repo` and returns the branch it was given. The agent cannot
    /// launch (its task folder is a file), which fails after the workspace is saved.
    async fn started(app: &AppState, mut req: RunRequest) -> String {
        req.project = "repo".into();
        let key = if req.key.trim().is_empty() {
            "LIBRE-1".to_string()
        } else {
            req.key.clone()
        };
        std::fs::create_dir_all(app.config.home.join("tasks")).unwrap();
        std::fs::write(app.config.task_dir(&key), "").unwrap();
        let _ = start_run(State(app.clone()), Json(req)).await;
        app.store
            .workspace(&TaskKey::new(&key))
            .await
            .unwrap()
            .unwrap()
            .1
            .branch
    }

    const NAMES_IT: &str =
        r#"echo '{"is_error":false,"result":"{\"name\":\"task branch names\"}"}'"#;

    /// Precedence at start, free task (decision 65): a typed branch wins.
    #[tokio::test]
    async fn a_free_task_with_a_typed_branch_starts_on_it() {
        let (app, _) = repo_app(NAMES_IT).await;
        let mut req = run("");
        req.title = "Nombres de rama".into();
        req.free = true;
        req.branch = Some("feature/mine".into());
        assert_eq!(started(&app, req).await, "feature/mine");
    }

    /// Precedence at start, free task: a project template names the branch.
    #[tokio::test]
    async fn a_free_task_with_a_template_starts_on_the_templates_branch() {
        let (app, _) = repo_app(NAMES_IT).await;
        let settings = delilah_core::ProjectSettings {
            branch: Some("work/{slug}".into()),
            ..Default::default()
        };
        app.store
            .set_project_settings("repo", &settings)
            .await
            .unwrap();
        let mut req = run("");
        req.title = "Nombres de rama".into();
        req.free = true;
        assert_eq!(started(&app, req).await, "work/task-branch-names");
    }

    /// A free task that started ON a branch (typed or template) keeps main's behaviour
    /// for it: the first analysis renames the task but nothing moves or creates a branch.
    #[tokio::test]
    async fn a_free_task_started_on_a_branch_is_retitled_but_never_branched() {
        for typed in [true, false] {
            let (app, _) = repo_app(NAMES_IT).await;
            let mut req = run("");
            req.title = "Nombres de rama".into();
            req.free = true;
            if typed {
                req.branch = Some("feature/mine".into());
            } else {
                let settings = delilah_core::ProjectSettings {
                    branch: Some("work/{slug}".into()),
                    ..Default::default()
                };
                app.store
                    .set_project_settings("repo", &settings)
                    .await
                    .unwrap();
            }
            let branch = started(&app, req).await;
            let key = TaskKey::new("LIBRE-1");
            let (_, ws) = app.store.workspace(&key).await.unwrap().unwrap();
            // What Spec Kit's hook does: its own branch and a spec folder.
            git(&ws.path, &["checkout", "-q", "-b", "006-x"]);
            let folder = ws.path.join("specs/006-x");
            std::fs::create_dir_all(&folder).unwrap();
            std::fs::write(
                folder.join("spec.md"),
                "# Feature Specification: Real name\n",
            )
            .unwrap();
            let task = app.store.get_task(&key).await.unwrap().unwrap();
            crate::branching::recover(&app, &task).await;
            assert_eq!(
                crate::branching::ensure(&app, &key, crate::branching::Why::Guard).await,
                Ok(None)
            );
            let task = app.store.get_task(&key).await.unwrap().unwrap();
            assert_eq!(task.title, "Real name", "typed={typed}");
            assert_eq!(task.branch_error, None);
            let (_, ws) = app.store.workspace(&key).await.unwrap().unwrap();
            assert_eq!(ws.branch, branch, "typed={typed}");
            // The guard lets edits through: the task has its branch.
            let edit = serde_json::json!({ "tool_name": "Edit", "tool_input": { "file_path": "src/a.rs" } });
            assert_eq!(crate::branching::guard(&app, &key, &edit).await, None);
            assert_eq!(git(&ws.path, &["branch", "--show-current"]), "006-x");
        }
    }

    /// Precedence at start, free task with neither: no branch at all.
    #[tokio::test]
    async fn a_free_task_with_neither_starts_without_a_branch() {
        let (app, _) = repo_app(NAMES_IT).await;
        let mut req = run("");
        req.title = "Nombres de rama".into();
        req.free = true;
        assert_eq!(started(&app, req).await, "");
    }

    #[tokio::test]
    async fn a_jira_task_keeps_its_key_in_the_branch() {
        let (app, _) = repo_app(NAMES_IT).await;
        let mut req = run("PROJ-12");
        req.title = "Nombres de rama".into();
        assert_eq!(
            started(&app, req).await,
            "feature/PROJ-12-task-branch-names"
        );
    }

    #[tokio::test]
    async fn a_proposal_sent_back_keeps_the_key_case() {
        let (app, _) = repo_app("exit 1").await;
        let mut req = run("PROJ-12");
        req.title = "t".into();
        req.branch = Some("feature/feature/PROJ-12-retry payments".into());
        assert_eq!(started(&app, req).await, "feature/PROJ-12-retry-payments");
    }

    #[tokio::test]
    async fn a_typed_branch_is_normalised_and_unique_even_in_checkout_mode() {
        let (app, repo) = repo_app("exit 1").await;
        git(&repo, &["branch", "feature/x"]);
        let mut req = run("PROJ-3");
        req.title = "t".into();
        req.branch = Some("X".into());
        req.in_checkout = Some(true);
        assert_eq!(started(&app, req).await, "feature/x-2");
        assert_eq!(git(&repo, &["branch", "--show-current"]), "feature/x-2");
        assert!(git(&repo, &["branch", "--list", "feature/x"]).contains("feature/x"));
    }

    #[tokio::test]
    async fn a_typed_branch_with_nothing_usable_is_refused_before_anything_changes() {
        let (app, _) = repo_app("exit 1").await;
        let key = backlog_task(&app, "PROJ-5").await;
        let mut req = run("PROJ-5");
        req.project = "repo".into();
        req.branch = Some("///".into());
        let err = start_run(State(app.clone()), Json(req)).await.unwrap_err();
        assert_eq!(err.status, StatusCode::BAD_REQUEST, "{}", err.message);
        let task = app.store.get_task(&key).await.unwrap().unwrap();
        assert_eq!(task.status, TaskStatus::Queued);
    }

    #[tokio::test]
    async fn saving_mode_gives_the_first_session_the_plan_step_and_a_note() {
        let app = app().await;
        let (s, prompt) = first_session(&app, "PROJ-1", true, false, "/feature PROJ-1".into());
        assert_eq!(s.step, Some(Step::Plan));
        assert_eq!(s.label, "Spec y plan");
        assert!(s.primary);
        assert!(prompt.starts_with("/feature PROJ-1 "), "{prompt}");
        assert!(
            prompt.ends_with("Delilah seguirá en una sesión nueva."),
            "{prompt}"
        );

        app.lang.set(crate::lang::Lang::En);
        let (s, prompt) = first_session(&app, "PROJ-1", true, false, "/feature PROJ-1".into());
        assert_eq!(s.label, "Spec and plan");
        assert!(prompt.contains("tasks.md"), "{prompt}");
        assert!(!prompt.ends_with("nueva."), "{prompt}");

        let (s, prompt) = first_session(&app, "PROJ-1", false, false, "/feature PROJ-1".into());
        assert_eq!((s.label.as_str(), s.step), ("Main", None));
        assert_eq!(prompt, "/feature PROJ-1");
        app.lang.set(crate::lang::Lang::Es);
        let (s, _) = first_session(&app, "PROJ-1", false, false, String::new());
        assert_eq!(s.label, "Principal");
    }

    #[tokio::test]
    async fn saving_mode_comes_from_the_request_else_the_preference() {
        let app = app().await;
        // The start fails (no folder), but the task keeps what it was started with.
        let saving = |app: &AppState, key: &str| {
            let key = TaskKey::new(key);
            let store = app.store.clone();
            async move { store.get_task(&key).await.unwrap().unwrap().saving_mode }
        };
        let mut req = run("PROJ-1");
        req.title = "a".into();
        start_run(State(app.clone()), Json(req)).await.unwrap_err();
        assert!(!saving(&app, "PROJ-1").await, "off by default");

        app.store
            .set_setting("pref:saving-mode", &serde_json::json!(true))
            .await
            .unwrap();
        let mut req = run("PROJ-2");
        req.title = "b".into();
        start_run(State(app.clone()), Json(req)).await.unwrap_err();
        assert!(saving(&app, "PROJ-2").await, "the preference");

        let mut req = run("PROJ-3");
        req.title = "c".into();
        req.saving_mode = Some(false);
        start_run(State(app.clone()), Json(req)).await.unwrap_err();
        assert!(!saving(&app, "PROJ-3").await, "the request wins");

        // A backlog task keeps its own choice, and goes back to it if the start fails.
        let mut task = Task::new(TaskKey::new("PROJ-4"), "d", "feature");
        task.project = Some("web".into());
        task.saving_mode = true;
        app.store.create_task(&task).await.unwrap();
        start_run(State(app.clone()), Json(run("PROJ-4")))
            .await
            .unwrap_err();
        assert!(saving(&app, "PROJ-4").await);
        let mut req = run("PROJ-4");
        req.saving_mode = Some(false);
        start_run(State(app.clone()), Json(req)).await.unwrap_err();
        assert!(saving(&app, "PROJ-4").await, "put back as it was");
    }

    #[tokio::test]
    async fn autopilot_replaces_the_saving_mode_note_in_both_languages() {
        let app = app().await;
        let (s, prompt) = first_session(&app, "PROJ-1", true, true, "/feature PROJ-1".into());
        assert_eq!(s.step, Some(Step::Plan));
        assert!(prompt.starts_with("/feature PROJ-1 "), "{prompt}");
        assert!(prompt.contains("Autopilot"), "{prompt}");
        assert!(prompt.contains("tasks.md"), "{prompt}");
        assert!(!prompt.contains("Modo ahorro:"), "{prompt}");
        assert!(prompt.contains("(Recomendado)"), "{prompt}");
        app.lang.set(crate::lang::Lang::En);
        let (_, prompt) = first_session(&app, "PROJ-1", true, true, "/feature PROJ-1".into());
        assert!(prompt.contains("Autopilot"), "{prompt}");
        assert!(prompt.contains("(Recommended)"), "{prompt}");
        assert!(!prompt.contains("Saving mode:"), "{prompt}");
    }

    #[tokio::test]
    async fn a_run_resolves_ci_autofix_from_the_request_then_the_project_default() {
        use delilah_core::AutofixDefault;
        let app = app().await;
        let on = |key: &str| {
            let key = TaskKey::new(key);
            let store = app.store.clone();
            async move { store.get_task(&key).await.unwrap().unwrap().ci_autofix }
        };
        // The project default is `autopilot`: on for an autopilot task only.
        let mut req = run("PROJ-1");
        req.title = "a".into();
        req.autopilot = Some(true);
        start_run(State(app.clone()), Json(req)).await.unwrap_err();
        assert!(on("PROJ-1").await, "autopilot task, default autopilot");
        let mut req = run("PROJ-2");
        req.title = "b".into();
        start_run(State(app.clone()), Json(req)).await.unwrap_err();
        assert!(!on("PROJ-2").await, "plain task, default autopilot");

        // The explicit value wins both ways.
        let mut req = run("PROJ-3");
        req.title = "c".into();
        req.ci_autofix = Some(true);
        start_run(State(app.clone()), Json(req)).await.unwrap_err();
        assert!(on("PROJ-3").await);
        let mut req = run("PROJ-4");
        req.title = "d".into();
        req.autopilot = Some(true);
        req.ci_autofix = Some(false);
        start_run(State(app.clone()), Json(req)).await.unwrap_err();
        assert!(!on("PROJ-4").await);

        // `off` turns the default off even for autopilot.
        let mut project = app.store.project("web").await.unwrap();
        project.settings.ci_autofix = AutofixDefault::Off;
        app.store
            .set_project_settings("web", &project.settings)
            .await
            .unwrap();
        let mut req = run("PROJ-5");
        req.title = "e".into();
        req.autopilot = Some(true);
        start_run(State(app.clone()), Json(req)).await.unwrap_err();
        assert!(!on("PROJ-5").await, "default off");

        // A backlog task keeps its own value unless the request sets one.
        let mut task = Task::new(TaskKey::new("PROJ-6"), "f", "feature");
        task.project = Some("web".into());
        task.ci_autofix = true;
        app.store.create_task(&task).await.unwrap();
        start_run(State(app.clone()), Json(run("PROJ-6")))
            .await
            .unwrap_err();
        assert!(on("PROJ-6").await, "kept");
        let mut req = run("PROJ-6");
        req.ci_autofix = Some(false);
        start_run(State(app.clone()), Json(req)).await.unwrap_err();
        assert!(!on("PROJ-6").await, "set by the request");
    }

    #[tokio::test]
    async fn autopilot_comes_from_the_request_and_implies_saving_mode() {
        let app = app().await;
        let get = |key: &str| {
            let key = TaskKey::new(key);
            let store = app.store.clone();
            async move { store.get_task(&key).await.unwrap().unwrap() }
        };
        let mut req = run("PROJ-1");
        req.title = "a".into();
        req.autopilot = Some(true);
        start_run(State(app.clone()), Json(req)).await.unwrap_err();
        let task = get("PROJ-1").await;
        assert!(task.saving_mode);
        assert_eq!(
            task.autopilot.map(|a| a.state),
            Some(delilah_core::AutopilotState::Running)
        );

        let mut req = run("PROJ-2");
        req.title = "b".into();
        req.autopilot = Some(true);
        req.saving_mode = Some(false);
        let err = start_run(State(app.clone()), Json(req)).await.unwrap_err();
        assert_eq!(err.status, StatusCode::BAD_REQUEST);
        assert!(
            app.store
                .get_task(&TaskKey::new("PROJ-2"))
                .await
                .unwrap()
                .is_none()
        );

        // Only the spec workflow can have it.
        let mut req = run("PROJ-9");
        req.title = "x".into();
        req.autopilot = Some(true);
        req.workflow = Some("prompt".into());
        req.prompt = Some("do it".into());
        let err = start_run(State(app.clone()), Json(req)).await.unwrap_err();
        assert_eq!(err.status, StatusCode::CONFLICT);
        assert_eq!(err.reason.as_deref(), Some("not-spec-workflow"));

        // Off by default.
        let mut req = run("PROJ-3");
        req.title = "c".into();
        start_run(State(app.clone()), Json(req)).await.unwrap_err();
        assert!(get("PROJ-3").await.autopilot.is_none());

        // A backlog task keeps its autopilot, and it goes back if the start fails.
        let mut task = Task::new(TaskKey::new("PROJ-4"), "d", "feature");
        task.project = Some("web".into());
        task.saving_mode = true;
        task.autopilot = Some(new_autopilot());
        let original = task.autopilot.clone();
        app.store.create_task(&task).await.unwrap();
        start_run(State(app.clone()), Json(run("PROJ-4")))
            .await
            .unwrap_err();
        assert_eq!(get("PROJ-4").await.autopilot, original, "kept");
        let mut req = run("PROJ-4");
        req.autopilot = Some(false);
        start_run(State(app.clone()), Json(req)).await.unwrap_err();
        assert_eq!(
            get("PROJ-4").await.autopilot,
            original,
            "put back as it was"
        );

        // A backlog task gets it from the request.
        let key = backlog_task(&app, "PROJ-5").await;
        let mut req = run("PROJ-5");
        req.autopilot = Some(true);
        start_run(State(app.clone()), Json(req)).await.unwrap_err();
        // Failing start restores the original (no autopilot, saving mode off).
        let task = app.store.get_task(&key).await.unwrap().unwrap();
        assert!(task.autopilot.is_none() && !task.saving_mode);
    }

    #[tokio::test]
    async fn a_backlog_task_that_cannot_start_stays_in_the_backlog() {
        let app = app().await;
        let key = backlog_task(&app, "PROJ-1").await;

        let err = start_run(State(app.clone()), Json(run("PROJ-1")))
            .await
            .unwrap_err();
        assert_eq!(
            err.status,
            StatusCode::UNPROCESSABLE_ENTITY,
            "{}",
            err.message
        );

        let task = app.store.get_task(&key).await.unwrap().unwrap();
        assert_eq!(task.status, TaskStatus::Queued);
        assert_eq!(task.title, "Reintentos", "the title comes from the task");
        let events: Vec<_> = app.store.events(&key).await.unwrap();
        assert_eq!(events.last().unwrap().event.name(), "start-aborted");

        // A new task started directly still fails, as before.
        let mut fresh = run("PROJ-2");
        fresh.title = "Otra".into();
        start_run(State(app.clone()), Json(fresh))
            .await
            .unwrap_err();
        let task = app
            .store
            .get_task(&TaskKey::new("PROJ-2"))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(task.status.name(), "failed");
    }

    #[tokio::test]
    async fn a_failed_start_leaves_the_backlog_task_as_it_was() {
        let app = app().await;
        let mut task = Task::new(TaskKey::new("PROJ-1"), "Reintentos", "feature");
        task.project = Some("web".into());
        task.notes = Some("con backoff".into());
        task.in_checkout = Some(false);
        app.store.create_task(&task).await.unwrap();

        // No project, title or notes: the task's own; then edited ones, put back.
        let req: RunRequest =
            serde_json::from_value(serde_json::json!({ "key": "PROJ-1" })).unwrap();
        start_run(State(app.clone()), Json(req)).await.unwrap_err();
        let mut edited = run("PROJ-1");
        edited.title = "Otro título".into();
        edited.notes = Some(String::new());
        edited.workflow = Some("prompt".into());
        edited.prompt = Some("hazlo".into());
        start_run(State(app.clone()), Json(edited))
            .await
            .unwrap_err();

        let after = app.store.get_task(&task.key).await.unwrap().unwrap();
        assert_eq!(after, task, "unchanged, still in the backlog");
    }

    #[tokio::test]
    async fn a_new_task_needs_a_project() {
        let app = app().await;
        let req: RunRequest =
            serde_json::from_value(serde_json::json!({ "key": "PROJ-9", "title": "x" })).unwrap();
        let err = start_run(State(app), Json(req)).await.unwrap_err();
        assert_eq!(err.status, StatusCode::BAD_REQUEST, "{}", err.message);
    }

    #[tokio::test]
    async fn a_new_task_keeps_the_notes_it_was_started_with() {
        let app = app().await;
        let mut req = run("PROJ-2");
        req.title = "Otra".into();
        req.notes = Some("mirar el TTL".into());
        start_run(State(app.clone()), Json(req)).await.unwrap_err();
        let task = app
            .store
            .get_task(&TaskKey::new("PROJ-2"))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(task.notes.as_deref(), Some("mirar el TTL"));
    }

    #[tokio::test]
    async fn free_keys_created_at_once_are_all_different() {
        let app = app().await;
        let keys = futures_util::future::join_all((0..8).map(|_| next_free_key(&app))).await;
        let mut keys: Vec<String> = keys.into_iter().map(|k| k.ok().unwrap()).collect();
        keys.sort();
        keys.dedup();
        assert_eq!(keys.len(), 8, "{keys:?}");
    }

    #[tokio::test]
    async fn only_backlog_tasks_can_be_started_again() {
        let app = app().await;
        let key = backlog_task(&app, "PROJ-1").await;
        app.store
            .apply_event(&key, Event::Start, chrono::Utc::now())
            .await
            .unwrap();
        let err = start_run(State(app.clone()), Json(run("PROJ-1")))
            .await
            .unwrap_err();
        assert_eq!(err.status, StatusCode::CONFLICT, "{}", err.message);

        let err = start_run(State(app), Json(run("PROJ-404")))
            .await
            .unwrap_err();
        assert_eq!(
            err.status,
            StatusCode::BAD_REQUEST,
            "a new task needs a title"
        );
    }

    #[tokio::test]
    async fn notes_reach_the_first_prompt_as_a_file() {
        let app = app().await;
        let key = TaskKey::new("PROJ-1");
        let prompt = point_to_notes(&app, &key, "/feature PROJ-1".into(), Some("con backoff"));
        let path = app.config.task_dir("PROJ-1").join("notes.md");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "con backoff");
        assert!(prompt.starts_with("/feature PROJ-1 ("), "{prompt}");
        assert!(
            prompt.ends_with(&format!("{})", path.display())),
            "{prompt}"
        );

        let bare = point_to_notes(&app, &key, "/feature PROJ-1".into(), Some("  "));
        assert_eq!(bare, "/feature PROJ-1");
    }

    #[tokio::test]
    async fn a_started_task_from_a_conversation_reads_the_edited_brief() {
        use tower::ServiceExt;
        let call = |app: &AppState, req: axum::http::Request<axum::body::Body>| {
            let router = crate::api::router(app.clone());
            async move {
                let res = router.oneshot(req).await.unwrap();
                let status = res.status();
                let body = axum::body::to_bytes(res.into_body(), usize::MAX)
                    .await
                    .unwrap();
                (
                    status,
                    serde_json::from_slice::<serde_json::Value>(&body).unwrap_or_default(),
                )
            }
        };
        let json_req = |method: &str, path: &str, body: serde_json::Value| {
            axum::http::Request::builder()
                .method(method)
                .uri(path)
                .header("content-type", "application/json")
                .body(axum::body::Body::from(body.to_string()))
                .unwrap()
        };

        // A real repo to work in, and a conversation of the project.
        let tmp = tempfile::TempDir::new().unwrap();
        let home = tmp.path().to_path_buf();
        let repo = home.join("web");
        // Stands in for `claude`: records the arguments it got, then goes away.
        let fake = home.join("fake-claude.sh");
        let seen = home.join("args.txt");
        std::fs::write(
            &fake,
            format!(
                "#!/bin/sh\nprintf '%s\\n' \"$@\" > '{}'\nsleep 1\n",
                seen.display()
            ),
        )
        .unwrap();
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        std::fs::create_dir_all(&repo).unwrap();
        git(&repo, &["init", "-q", "-b", "main"]);
        std::fs::write(repo.join("README.md"), "web\n").unwrap();
        git(&repo, &["add", "."]);
        git(&repo, &["commit", "-qm", "init"]);
        let config = crate::config::Config {
            home: home.clone(),
            addr: crate::config::DEFAULT_ADDR.parse().unwrap(),
            allow_file_protocol: false,
            notify: false,
            ui_origin: None,
            agent_program: Some(fake),
            ..crate::config::Config::test_defaults()
        };
        let app = AppState::new(crate::store::Store::in_memory().await.unwrap(), config);
        app.lang.set(crate::lang::Lang::Es); // Spanish fixtures
        app.store
            .create_project(&delilah_core::Project {
                name: "web".into(),
                path: repo,
                kind: ProjectKind::Repo,
                workflow: "feature".into(),
                settings: Default::default(),
                preview_command: None,
            })
            .await
            .unwrap();
        let mut conv = session("@web", "Idea");
        conv.id = "conv".into();
        conv.primary = false;
        conv.conversation = true;
        app.store.create_session(&conv).await.unwrap();

        let task = crate::api::build_task(
            &app,
            crate::api::NewTask {
                key: String::new(),
                title: "Reintentos".into(),
                project: Some("web".into()),
                notes: Some("brief original".into()),
                in_checkout: None,
                saving_mode: None,
                autopilot: None,
                workflow: None,
                free: true,
                view: None,
                ci_autofix: None,
            },
        )
        .await
        .map_err(|e| e.message)
        .unwrap();
        app.store
            .create_task_from_conversation(&task, "conv", "Idea", chrono::Utc::now())
            .await
            .unwrap();
        let key = task.key.to_string();

        let (status, edited) = call(
            &app,
            json_req(
                "PATCH",
                &format!("/api/v1/tasks/{key}"),
                serde_json::json!({ "notes": "brief editado" }),
            ),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{edited}");

        let (status, started) = call(
            &app,
            json_req(
                "POST",
                "/api/v1/runs",
                serde_json::json!({ "key": key, "title": "", "project": "web" }),
            ),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED, "{started}");

        let path = app.config.task_dir(&key).join("notes.md");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "brief editado");
        // What the program received, not what we would have built.
        let mut args = String::new();
        for _ in 0..100 {
            args = std::fs::read_to_string(&seen).unwrap_or_default();
            if !args.is_empty() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
        for live in app.sessions.of_task(&key) {
            let _ = live.runtime.kill();
        }
        assert!(
            args.contains(&format!("{})", path.display())),
            "the prompt does not point to notes.md: {args}"
        );
    }

    /// A daemon whose `claude` is `script`, a task `PROJ-1` (chat mode as asked) and a
    /// workspace folder for it.
    async fn chat_app(script: &str, chat_mode: bool) -> (AppState, std::path::PathBuf) {
        use std::os::unix::fs::PermissionsExt;
        let app = app().await;
        let ws = app.config.home.join("ws");
        std::fs::create_dir_all(&ws).unwrap();
        let bin = app.config.home.join("fake-claude.sh");
        std::fs::write(&bin, format!("#!/bin/sh\n{script}\n")).unwrap();
        std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
        let mut config = app.config.as_ref().clone();
        config.agent_program = Some(bin);
        let app = AppState::new(app.store.clone(), config);
        app.lang.set(crate::lang::Lang::Es); // Spanish fixtures
        let mut task = Task::new(TaskKey::new("PROJ-1"), "t", "feature");
        task.project = Some("web".into());
        task.chat_mode = chat_mode;
        app.store.create_task(&task).await.unwrap();
        (app, ws)
    }

    /// Acts like the real stream agent: saves its arguments, says nothing but a hook line
    /// until it reads its first message, saves it and then prints its `system/init`.
    fn hello_script(home_args: &str) -> String {
        format!(
            "printf '%s\\n' \"$@\" > '{home_args}/args.txt'\n\
             echo '{{\"type\":\"system\",\"subtype\":\"hook_started\"}}'\n\
             read line; printf '%s' \"$line\" > '{home_args}/stdin.txt'\n\
             echo '{{\"type\":\"system\",\"subtype\":\"init\",\"session_id\":\"sess-1\"}}'\n\
             sleep 5"
        )
    }

    async fn read_when_there(path: std::path::PathBuf) -> String {
        for _ in 0..100 {
            if let Ok(text) = std::fs::read_to_string(&path)
                && !text.is_empty()
            {
                return text;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        String::new()
    }

    #[tokio::test]
    async fn a_chat_mode_task_launches_a_stream_with_the_prompt_on_stdin() {
        let home = std::env::temp_dir().join(format!("delilah-chat-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&home).unwrap();
        let (app, ws) = chat_app(&hello_script(&home.display().to_string()), true).await;
        let record = session("PROJ-1", "Main");
        launch_session_with(
            &app,
            &record,
            &ws,
            Some("do the thing"),
            false,
            Duration::from_secs(10),
        )
        .await
        .map_err(|e| e.message)
        .unwrap();
        let live = app.sessions.get("sess-1").unwrap();
        assert!(live.runtime.as_stream().is_some() && live.runtime.as_pty().is_none());
        let stdin = read_when_there(home.join("stdin.txt")).await;
        assert!(stdin.contains("do the thing"), "{stdin}");
        let args = std::fs::read_to_string(home.join("args.txt")).unwrap();
        assert!(
            args.contains("stream-json") && args.contains("--session-id"),
            "{args}"
        );
        assert!(
            !args.contains("do the thing"),
            "no positional prompt: {args}"
        );
        let _ = live.runtime.kill();
    }

    #[tokio::test]
    async fn the_stderr_of_a_stream_agent_goes_to_the_session_log() {
        let script = "echo 'to the log' >&2\nread line\necho '{\"type\":\"system\",\"subtype\":\"init\",\"session_id\":\"sess-1\"}'\nsleep 5";
        let (app, ws) = chat_app(script, true).await;
        launch_session_with(
            &app,
            &session("PROJ-1", "Main"),
            &ws,
            Some("hi"),
            false,
            Duration::from_secs(10),
        )
        .await
        .map_err(|e| e.message)
        .unwrap();
        let log = app.config.task_dir("PROJ-1").join("claude-sess-1.log");
        assert_eq!(read_when_there(log).await, "to the log\n");
        let _ = app.sessions.get("sess-1").unwrap().runtime.kill();
    }

    #[tokio::test]
    async fn project_sessions_and_conversations_and_plain_tasks_launch_a_pty() {
        let (app, ws) = chat_app("sleep 5", true).await;
        // A project's Claude and an idea conversation.
        let project = SessionRecord {
            id: "p-1".into(),
            ..session("@web", "Claude 1")
        };
        let idea = SessionRecord {
            id: "i-1".into(),
            conversation: true,
            ..session("@web", "Idea")
        };
        // A conversation is a terminal even on a task that has the flag.
        let task_idea = SessionRecord {
            id: "t-1".into(),
            conversation: true,
            ..session("PROJ-1", "Idea")
        };
        for record in [&project, &idea, &task_idea] {
            assert_eq!(
                runtime_kind(&app, record)
                    .await
                    .map_err(|e| e.message)
                    .unwrap(),
                RuntimeKind::Pty
            );
        }
        launch_session_with(&app, &project, &ws, None, false, Duration::from_secs(10))
            .await
            .map_err(|e| e.message)
            .unwrap();
        let live = app.sessions.get("p-1").unwrap();
        assert!(live.runtime.as_pty().is_some());
        let _ = live.runtime.kill();

        // Without the flag, a task's session is a terminal too.
        let (app, _) = chat_app("sleep 5", false).await;
        let main = session("PROJ-1", "Main");
        assert_eq!(
            runtime_kind(&app, &main)
                .await
                .map_err(|e| e.message)
                .unwrap(),
            RuntimeKind::Pty
        );
    }

    #[tokio::test]
    async fn an_agent_that_exits_before_init_fails_the_start_with_its_stderr() {
        let (app, ws) = chat_app("echo 'no such flag' >&2\nexit 2", true).await;
        let err = launch_session_with(
            &app,
            &session("PROJ-1", "Main"),
            &ws,
            Some("hi"),
            false,
            Duration::from_secs(10),
        )
        .await
        .unwrap_err();
        assert_eq!(err.message, "chat mode could not start: no such flag");
        // Nothing stays registered, and the task is as it was.
        for _ in 0..100 {
            if app.sessions.get("sess-1").is_none() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        assert!(app.sessions.get("sess-1").is_none());
        let task = app
            .store
            .get_task(&TaskKey::new("PROJ-1"))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(task.status, TaskStatus::Queued);
    }

    #[tokio::test]
    async fn an_agent_that_stays_silent_after_the_first_message_times_out() {
        let (app, ws) = chat_app("sleep 5", true).await;
        let err = launch_session_with(
            &app,
            &session("PROJ-1", "Main"),
            &ws,
            Some("hi"),
            false,
            Duration::from_millis(300),
        )
        .await
        .unwrap_err();
        assert!(
            err.message.starts_with("chat mode could not start:"),
            "{}",
            err.message
        );
        // Nothing stays registered.
        assert!(app.sessions.get("sess-1").is_none());
    }

    #[tokio::test]
    async fn an_agent_that_exits_after_the_first_message_without_a_word_fails_the_start() {
        let (app, ws) = chat_app("read line\necho 'boom' >&2\nexit 1", true).await;
        let err = launch_session_with(
            &app,
            &session("PROJ-1", "Main"),
            &ws,
            Some("hi"),
            false,
            Duration::from_secs(10),
        )
        .await
        .unwrap_err();
        assert_eq!(err.message, "chat mode could not start: boom");
        assert!(app.sessions.get("sess-1").is_none());
    }

    #[tokio::test]
    async fn a_relaunch_without_a_prompt_is_up_when_the_agent_stays_alive() {
        let (app, ws) = chat_app("sleep 5", true).await;
        let record = session("PROJ-1", "Main");
        launch_session_with(&app, &record, &ws, None, false, Duration::from_secs(10))
            .await
            .map_err(|e| e.message)
            .unwrap();
        let live = app.sessions.get("sess-1").expect("registered");
        let _ = live.runtime.kill();
        // And one that dies at once is not: with a settle far longer than it takes to die,
        // so a slow machine cannot make it look up.
        let (app, ws) = chat_app("exit 1", true).await;
        let err = spawn_session(
            &app,
            &record,
            &ws,
            None,
            false,
            Duration::from_secs(30),
            Duration::from_secs(30),
            false,
        )
        .await
        .map(|_| ())
        .unwrap_err();
        assert!(err.message.starts_with("chat mode could not start:"));
    }

    #[tokio::test]
    async fn a_first_message_that_cannot_be_sent_kills_the_agent_and_forgets_the_session() {
        // The agent closes its stdin and stays up: typing the first message fails.
        let (app, ws) = chat_app("exec 0<&-\nsleep 5", true).await;
        let record = session("PROJ-1", "Main");
        let launched = launch_session_with(
            &app,
            &record,
            &ws,
            Some("hi"),
            false,
            Duration::from_secs(10),
        )
        .await;
        let err = launched.unwrap_err();
        assert!(
            err.message.contains("cannot send the first message")
                || err.message.starts_with("chat mode could not start"),
            "{}",
            err.message
        );
        assert!(app.sessions.get("sess-1").is_none(), "no live session left");
    }

    #[tokio::test]
    async fn a_relaunch_of_a_chat_mode_task_resumes_on_a_stream() {
        let home = std::env::temp_dir().join(format!("delilah-chat-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&home).unwrap();
        let (app, ws) = chat_app(&hello_script(&home.display().to_string()), true).await;
        // A saved conversation, so `--resume` is used; kept under the daemon's projects folder.
        let projects = home.join("projects");
        let mut config = app.config.as_ref().clone();
        config.claude_projects = projects.clone();
        let app = AppState::new(app.store.clone(), config);
        app.lang.set(crate::lang::Lang::Es); // Spanish fixtures
        let record = SessionRecord {
            id: format!("resume-{}", uuid::Uuid::new_v4()),
            ..session("PROJ-1", "Main")
        };
        let transcript =
            delilah_agents::transcript::transcript_path_under(&projects, &ws, &record.id).unwrap();
        std::fs::create_dir_all(transcript.parent().unwrap()).unwrap();
        std::fs::write(&transcript, "{}\n").unwrap();
        let launched = launch_session_with(
            &app,
            &record,
            &ws,
            Some("go on"),
            true,
            Duration::from_secs(10),
        )
        .await;
        let args = read_when_there(home.join("args.txt")).await;
        if let Some(live) = app.sessions.get(&record.id) {
            assert!(live.runtime.as_stream().is_some());
            let _ = live.runtime.kill();
        }
        let _ = std::fs::remove_dir_all(&home);
        launched.map_err(|e| e.message).unwrap();
        assert!(
            args.contains("--resume") && args.contains(&record.id),
            "{args}"
        );
        assert!(args.contains("stream-json"), "{args}");
    }

    #[test]
    fn the_start_error_is_short_and_hides_keys_and_tokens() {
        let long = "x".repeat(1000);
        let tail = start_error_tail(&format!(
            "ANTHROPIC_API_KEY=sk-ant-abc123 failed\nAuthorization: Bearer eyJhbGci.zzz\n{long}"
        ));
        assert!(
            !tail.contains("abc123") && !tail.contains("eyJhbGci"),
            "{tail}"
        );
        assert!(tail.contains("ANTHROPIC_API_KEY=sk-***") && tail.contains("Bearer ***"));
        let many = (0..100)
            .map(|n| format!("line {n} {}", "y".repeat(280)))
            .collect::<Vec<_>>()
            .join("\n");
        let tail = start_error_tail(&many);
        assert!(tail.len() <= START_ERROR_MAX, "{}", tail.len());
        assert!(tail.ends_with(&"y".repeat(20)));
        assert!(start_error_tail("x".repeat(1000).as_str()).len() <= START_ERROR_LINE_MAX);
    }

    #[tokio::test]
    async fn a_chat_mode_start_that_fails_leaves_the_task_startable_in_the_terminal() {
        use std::os::unix::fs::PermissionsExt;
        let (app, _repo) = repo_app("exit 1").await;
        let bin = app.config.home.join("fake-agent.sh");
        let made = app.config.home.join("pty-started");
        // A stream start dies at once; a terminal one leaves a file and stays up.
        std::fs::write(
            &bin,
            format!(
                "#!/bin/sh\ncase \"$*\" in *stream-json*) echo 'no stream here' >&2; exit 2;; esac\necho ran > '{}'\nsleep 5\n",
                made.display()
            ),
        )
        .unwrap();
        std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
        let mut config = app.config.as_ref().clone();
        config.agent_program = Some(bin);
        let app = AppState::new(app.store.clone(), config);
        app.lang.set(crate::lang::Lang::Es); // Spanish fixtures
        let mut task = Task::new(TaskKey::new("LIBRE-1"), "Reintentos", "feature");
        task.project = Some("repo".into());
        task.in_checkout = Some(false);
        task.chat_mode = true;
        app.store.create_task(&task).await.unwrap();
        let mut req = run("LIBRE-1");
        req.project = "repo".into();
        let mut retry = run("LIBRE-1");
        retry.project = "repo".into();

        let err = start_run(State(app.clone()), Json(req)).await.unwrap_err();
        assert!(
            err.message.starts_with("chat mode could not start:"),
            "{}",
            err.message
        );
        let key = TaskKey::new("LIBRE-1");
        let after = app.store.get_task(&key).await.unwrap().unwrap();
        assert_eq!(after, task, "back in the backlog, as it was");
        assert!(app.store.workspace(&key).await.unwrap().is_none());
        assert!(
            app.store
                .primary_session("LIBRE-1")
                .await
                .unwrap()
                .is_none()
        );

        // The retry in the terminal (what "Iniciar en terminal" sends).
        retry.chat_mode = Some(false);
        let (status, Json(started)) = start_run(State(app.clone()), Json(retry))
            .await
            .map_err(|e| e.message)
            .unwrap();
        assert_eq!(status, StatusCode::CREATED);
        assert!(!started.chat_mode);
        assert_eq!(started.status, TaskStatus::Running);
        let live = app.sessions.primary("LIBRE-1").expect("a live session");
        assert!(live.runtime.as_pty().is_some());
        assert!(
            !read_when_there(made).await.is_empty(),
            "the terminal agent ran"
        );
        let _ = live.runtime.kill();
    }

    /// A daemon with a git project `repo` whose agent (`claude`) runs `script`.
    async fn chat_repo_app(script: &str) -> AppState {
        use std::os::unix::fs::PermissionsExt;
        let (app, _repo) = repo_app("exit 1").await;
        let bin = app.config.home.join("fake-agent.sh");
        std::fs::write(&bin, format!("#!/bin/sh\n{script}\n")).unwrap();
        std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
        let mut config = app.config.as_ref().clone();
        config.agent_program = Some(bin);
        AppState::new(app.store.clone(), config)
    }

    fn chat_run(key: &str) -> RunRequest {
        let mut req = run(key);
        req.title = "Chat".into();
        req.project = "repo".into();
        req.chat_mode = Some(true);
        req.in_checkout = Some(false);
        req
    }

    #[tokio::test]
    async fn a_chat_mode_start_gives_the_first_message_to_a_running_task() {
        let home = std::env::temp_dir().join(format!("delilah-chat-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&home).unwrap();
        let app = chat_repo_app(&hello_script(&home.display().to_string())).await;
        let (status, Json(started)) = start_run(State(app.clone()), Json(chat_run("LIBRE-1")))
            .await
            .map_err(|e| e.message)
            .unwrap();
        assert_eq!(status, StatusCode::CREATED);
        assert!(started.chat_mode);
        assert_eq!(started.status, TaskStatus::Running);
        let stdin = read_when_there(home.join("stdin.txt")).await;
        assert!(stdin.contains("/feature"), "{stdin}");
        let live = app.sessions.primary("LIBRE-1").expect("a live session");
        assert!(live.runtime.as_stream().is_some());
        let _ = live.runtime.kill();
    }

    #[tokio::test]
    async fn a_chat_agent_that_dies_on_the_first_message_leaves_the_task_startable_and_no_session()
    {
        // A stream agent dies on its first message; a terminal one (the retry) stays up.
        let made = std::env::temp_dir().join(format!("delilah-pty-{}", uuid::Uuid::new_v4()));
        let app = chat_repo_app(&format!(
            "case \"$*\" in *stream-json*) read line; echo 'no credit' >&2; exit 1;; esac\necho ran > '{}'\nsleep 5",
            made.display()
        ))
        .await;
        let err = start_run(State(app.clone()), Json(chat_run("LIBRE-1")))
            .await
            .unwrap_err();
        assert_eq!(err.message, "chat mode could not start: no credit");
        let key = TaskKey::new("LIBRE-1");
        let task = app.store.get_task(&key).await.unwrap().unwrap();
        assert_eq!(task.status, TaskStatus::Queued, "back in the backlog");
        assert!(
            app.store
                .primary_session("LIBRE-1")
                .await
                .unwrap()
                .is_none()
        );
        assert!(app.store.workspace(&key).await.unwrap().is_none());
        assert!(app.sessions.primary("LIBRE-1").is_none());

        // "Iniciar en terminal": the same task, chat mode off.
        let mut retry = chat_run("LIBRE-1");
        retry.chat_mode = Some(false);
        let (status, Json(started)) = start_run(State(app.clone()), Json(retry))
            .await
            .map_err(|e| e.message)
            .unwrap();
        assert_eq!(status, StatusCode::CREATED);
        assert!(!started.chat_mode);
        assert_eq!(started.status, TaskStatus::Running);
        let live = app.sessions.primary("LIBRE-1").expect("a live session");
        assert!(live.runtime.as_pty().is_some());
        assert!(!read_when_there(made).await.is_empty());
        let _ = live.runtime.kill();
    }

    #[tokio::test]
    async fn an_error_result_as_the_first_word_fails_the_start_with_its_text() {
        let app = chat_repo_app(
            "read line\necho '{\"type\":\"result\",\"subtype\":\"success\",\"is_error\":true,\"result\":\"Invalid API key\"}'\nsleep 5",
        )
        .await;
        let err = start_run(State(app.clone()), Json(chat_run("LIBRE-1")))
            .await
            .unwrap_err();
        assert_eq!(err.message, "chat mode could not start: Invalid API key");
        let task = app.store.get_task(&TaskKey::new("LIBRE-1")).await.unwrap();
        assert_eq!(task.unwrap().status, TaskStatus::Queued);
        assert!(
            app.sessions.primary("LIBRE-1").is_none(),
            "the agent was stopped"
        );
    }

    #[tokio::test]
    async fn a_client_that_gives_up_mid_start_leaves_no_agent_and_a_startable_task() {
        // Reads its message and then says nothing: the start waits for a sign of work.
        let app = chat_repo_app("read line\nsleep 30").await;
        let mut waiting = Box::pin(start_run_with(
            app.clone(),
            chat_run("LIBRE-1"),
            Duration::from_secs(2),
            SETTLE,
        ));
        // The request is dropped once the agent is up and the start waits on it (what a
        // closed connection does).
        loop {
            tokio::select! {
                _ = &mut waiting => panic!("the start cannot finish: the agent says nothing"),
                () = tokio::time::sleep(Duration::from_millis(20)) => {
                    if app.sessions.primary("LIBRE-1").is_some() {
                        break;
                    }
                }
            }
        }
        drop(waiting);
        let key = TaskKey::new("LIBRE-1");
        for _ in 0..300 {
            let queued = app
                .store
                .get_task(&key)
                .await
                .unwrap()
                .is_some_and(|t| t.status == TaskStatus::Queued);
            if queued {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        let task = app.store.get_task(&key).await.unwrap().unwrap();
        assert_eq!(task.status, TaskStatus::Queued, "the rollback ran");
        assert!(app.sessions.primary("LIBRE-1").is_none(), "no live agent");
        assert!(
            app.store
                .primary_session("LIBRE-1")
                .await
                .unwrap()
                .is_none()
        );
    }

    #[tokio::test]
    async fn a_failed_chat_start_from_a_conversation_gives_it_back_to_its_project() {
        let app = chat_repo_app("echo 'bad flag' >&2\nexit 2").await;
        let original = SessionRecord {
            id: "conv-1".into(),
            ..session("@repo", "Claude 1")
        };
        let original = SessionRecord {
            primary: false,
            read_only: true,
            ..original
        };
        app.store.create_session(&original).await.unwrap();
        let mut req = chat_run("");
        req.from_session = Some("conv-1".into());
        // A settle far longer than the agent takes to die: it is seen dying, never taken
        // as up because the machine was slow to run it.
        let err = start_run_with(
            app.clone(),
            req,
            Duration::from_secs(30),
            Duration::from_secs(30),
        )
        .await
        .unwrap_err();
        assert!(
            err.message.starts_with("chat mode could not start:"),
            "{}",
            err.message
        );
        let after = app.store.session("conv-1").await.unwrap().unwrap();
        assert_eq!(after, original, "still the project's, as it was");
    }

    #[tokio::test]
    async fn an_adopted_start_that_dies_after_the_settle_is_seen_by_the_exit_watcher() {
        let (app, ws) = chat_app("sleep 0.5\nexit 1", true).await;
        let record = session("PROJ-1", "Main");
        // No first message (a resumed conversation): up after the settle alone.
        let spawned = spawn_session(
            &app,
            &record,
            &ws,
            None,
            true,
            Duration::from_secs(10),
            Duration::from_millis(100),
            false,
        )
        .await
        .map_err(|e| e.message)
        .unwrap();
        assert!(app.sessions.get("sess-1").is_some(), "judged up");
        spawned
            .send_first(&app)
            .await
            .map_err(|e| e.message)
            .unwrap();
        for _ in 0..100 {
            if app.sessions.get("sess-1").is_none() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        assert!(app.sessions.get("sess-1").is_none(), "gone once it died");
    }

    #[tokio::test]
    async fn a_stream_exit_with_a_held_stop_replays_it_after_stopping_the_rows() {
        let (app, ws) = chat_app("sleep 1.5\nexit 0", true).await;
        let key = TaskKey::new("PROJ-1");
        for event in [Event::Start, Event::WorkspaceReady] {
            app.transition(&key, event).await.unwrap();
        }
        let record = session("PROJ-1", "Main");
        app.store.create_session(&record).await.unwrap();
        spawn_session(
            &app,
            &record,
            &ws,
            None,
            true,
            Duration::from_secs(10),
            Duration::from_millis(100),
            false,
        )
        .await
        .map_err(|e| e.message)
        .unwrap();
        let live = app.sessions.get("sess-1").expect("up");
        live.set_state(crate::sessions::LiveState::Working);
        live.update_progress(|p| {
            crate::progress::apply(
                p,
                &delilah_agents::stream::StreamEvent::Task(
                    delilah_agents::stream::TaskEvent::Started {
                        task_id: "b1".into(),
                        tool_use_id: None,
                        kind: delilah_agents::stream::TaskKind::Command,
                        description: "sleep".into(),
                        background: true,
                    },
                ),
                chrono::Utc::now(),
            )
        });
        assert!(live.hold_stop_if_background(
            serde_json::json!({ "hook_event_name": "Stop", "last_assistant_message": "listo" })
        ));
        for _ in 0..100 {
            if app.sessions.get("sess-1").is_none() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        assert!(app.sessions.get("sess-1").is_none(), "gone once it exited");
        assert_eq!(
            live.state(),
            crate::sessions::LiveState::Waiting,
            "the Stop applied"
        );
        assert!(!live.has_held_stop());
        assert_eq!(live.progress().background[0].state, "stopped");
        assert_eq!(
            app.store.get_task(&key).await.unwrap().unwrap().status,
            TaskStatus::WaitingForUser
        );
    }

    #[tokio::test]
    async fn runs_accept_chat_mode_on_a_new_task() {
        let app = app().await;
        let mut req = run("PROJ-1");
        req.title = "a".into();
        req.chat_mode = Some(true);
        // The start fails (no folder), but the task keeps the flag.
        start_run(State(app.clone()), Json(req)).await.unwrap_err();
        let task = app
            .store
            .get_task(&TaskKey::new("PROJ-1"))
            .await
            .unwrap()
            .unwrap();
        assert!(task.chat_mode);
    }

    #[tokio::test]
    async fn free_tasks_start_branchless_and_jira_ones_on_their_feature_branch() {
        use tower::ServiceExt;
        let tmp = tempfile::TempDir::new().unwrap();
        let home = tmp.path().to_path_buf();
        let repo = home.join("web");
        let fake = home.join("fake-claude.sh");
        let seen = home.join("args.txt");
        std::fs::write(
            &fake,
            format!(
                "#!/bin/sh\nprintf '%s\\n' \"$@\" >> '{}'\nsleep 1\n",
                seen.display()
            ),
        )
        .unwrap();
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        std::fs::create_dir_all(&repo).unwrap();
        git(&repo, &["init", "-q", "-b", "main"]);
        std::fs::write(repo.join("README.md"), "web\n").unwrap();
        git(&repo, &["add", "."]);
        git(&repo, &["commit", "-qm", "init"]);
        let config = crate::config::Config {
            home: home.clone(),
            addr: crate::config::DEFAULT_ADDR.parse().unwrap(),
            allow_file_protocol: false,
            notify: false,
            ui_origin: None,
            // A failing one-shot: the Jira task is named from its title, never by a real Claude.
            claude_bin: "/usr/bin/false".into(),
            agent_program: Some(fake),
            ..crate::config::Config::test_defaults()
        };
        let app = AppState::new(crate::store::Store::in_memory().await.unwrap(), config);
        app.lang.set(crate::lang::Lang::Es); // Spanish fixtures
        app.store
            .create_project(&delilah_core::Project {
                name: "web".into(),
                path: repo,
                kind: ProjectKind::Repo,
                workflow: "feature".into(),
                preview_command: None,
                settings: Default::default(),
            })
            .await
            .unwrap();

        for (key, free) in [("LIBRE-1", true), ("PROJ-1", false)] {
            let req = axum::http::Request::builder()
                .method("POST")
                .uri("/api/v1/runs")
                .header("content-type", "application/json")
                .body(axum::body::Body::from(
                    serde_json::json!({ "key": key, "title": "Idea", "project": "web", "free": free, "in_checkout": false })
                        .to_string(),
                ))
                .unwrap();
            let res = crate::api::router(app.clone()).oneshot(req).await.unwrap();
            assert_eq!(res.status(), StatusCode::CREATED, "{key}");
        }
        let branch_of = |key: &str| {
            let app = app.clone();
            let key = TaskKey::new(key);
            async move { app.store.workspace(&key).await.unwrap().unwrap().1.branch }
        };
        assert_eq!(branch_of("LIBRE-1").await, "");
        // A Jira task is named at start from its title (decision 37).
        assert_eq!(branch_of("PROJ-1").await, "feature/PROJ-1-idea");

        let mut args = String::new();
        for _ in 0..100 {
            args = std::fs::read_to_string(&seen).unwrap_or_default();
            if args.contains("PROJ-1") && args.contains("Idea") {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
        for key in ["LIBRE-1", "PROJ-1"] {
            for live in app.sessions.of_task(key) {
                let _ = live.runtime.kill();
            }
        }
        // Only the free task's first prompt says it has no branch.
        assert_eq!(
            args.matches("let Spec Kit create it when specifying")
                .count()
                + args
                    .matches("deja que Spec Kit la cree al especificar")
                    .count(),
            1,
            "{args}"
        );
    }

    #[test]
    fn remote_names_show_the_task_or_project_and_the_label() {
        assert_eq!(
            remote_name(&session("PROJ-1", "Principal")),
            "PROJ-1 · Principal"
        );
        assert_eq!(remote_name(&session("@delilah", "Main")), "delilah · Main");
    }

    /// A `claude` that is the fake stream agent.
    fn fake_stream_script() -> String {
        format!(
            "exec bash {}/../delilah-agents/tests/fixtures/fake-claude-stream.sh",
            env!("CARGO_MANIFEST_DIR")
        )
    }

    async fn gone(app: &AppState, id: &str) -> bool {
        for _ in 0..100 {
            if app.sessions.get(id).is_none() {
                return true;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        false
    }

    #[tokio::test]
    async fn a_stream_that_dies_mid_turn_is_exited_like_a_pty_and_can_be_resumed_in_either_mode() {
        let (app, ws) = chat_app(&fake_stream_script(), true).await;
        let record = session("PROJ-1", "Main");
        launch_session_with(
            &app,
            &record,
            &ws,
            Some("NEEDS_PERMISSION"),
            false,
            Duration::from_secs(10),
        )
        .await
        .map_err(|e| e.message)
        .unwrap();
        let live = app.sessions.get("sess-1").expect("live");
        assert!(live.runtime.as_stream().is_some());
        for _ in 0..100 {
            if app.permissions.has_session("sess-1") {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        assert!(app.permissions.has_session("sess-1"), "the agent asked");
        // The process dies mid-turn, with the ask pending.
        let _ = live.runtime.kill();
        assert!(gone(&app, "sess-1").await, "marked exited");
        for _ in 0..100 {
            if !app.permissions.has_session("sess-1") {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        assert!(!app.permissions.has_session("sess-1"), "ask released");

        // Resumed on a stream (the task is in chat mode)...
        launch_session_with(&app, &record, &ws, None, true, Duration::from_secs(10))
            .await
            .map_err(|e| e.message)
            .unwrap();
        let live = app.sessions.get("sess-1").expect("live again");
        assert!(live.runtime.as_stream().is_some());
        let _ = live.runtime.kill();
        assert!(gone(&app, "sess-1").await);

        // ...and in the terminal once the task is switched.
        app.store
            .set_chat_mode(&TaskKey::new("PROJ-1"), false)
            .await
            .unwrap();
        launch_session_with(&app, &record, &ws, None, true, Duration::from_secs(10))
            .await
            .map_err(|e| e.message)
            .unwrap();
        let live = app.sessions.get("sess-1").expect("live in a pty");
        assert!(live.runtime.as_pty().is_some());
        let _ = live.runtime.kill();
    }

    #[tokio::test]
    async fn resuming_a_chat_mode_task_after_a_restart_relaunches_its_primary_on_a_stream() {
        let (app, ws) = chat_app(&fake_stream_script(), true).await;
        let key = TaskKey::new("PROJ-1");
        let workspace = delilah_git::Workspace {
            path: ws.clone(),
            branch: "b".into(),
            kind: ProjectKind::Repo,
            submodules: vec![],
        };
        app.store
            .save_workspace(&key, &ws, &workspace)
            .await
            .unwrap();
        app.store
            .create_session(&session("PROJ-1", "Main"))
            .await
            .unwrap();
        // No live session: a fresh daemon.
        assert!(app.sessions.get("sess-1").is_none());
        crate::actions::continue_agent(&app, &key)
            .await
            .map_err(|e| e.message)
            .unwrap();
        let live = app.sessions.get("sess-1").expect("relaunched");
        assert!(live.runtime.as_stream().is_some());
        let _ = live.runtime.kill();
    }

    /// A repo project with one commit and an untracked `.env`, with these settings.
    async fn settings_app(
        settings: delilah_core::ProjectSettings,
    ) -> (AppState, std::path::PathBuf) {
        let home = std::env::temp_dir().join(format!("delilah-run-{}", uuid::Uuid::new_v4()));
        let repo = home.join("web");
        std::fs::create_dir_all(&repo).unwrap();
        let git = |args: &[&str]| {
            let out = std::process::Command::new("git")
                .args([
                    "-c",
                    "user.name=t",
                    "-c",
                    "user.email=t@t",
                    "-c",
                    "commit.gpgsign=false",
                ])
                .args(args)
                .current_dir(&repo)
                .output()
                .unwrap();
            assert!(out.status.success(), "{args:?}: {out:?}");
        };
        git(&["init", "-b", "main"]);
        std::fs::write(repo.join("README.md"), "hi").unwrap();
        git(&["add", "."]);
        git(&["commit", "-m", "init"]);
        std::fs::write(repo.join(".git/info/exclude"), ".env\nran-setup\n").unwrap();
        std::fs::write(repo.join(".env"), "SECRET=1").unwrap();
        let config = crate::config::Config {
            home,
            addr: crate::config::DEFAULT_ADDR.parse().unwrap(),
            allow_file_protocol: false,
            notify: false,
            ui_origin: None,
            claude_bin: "/usr/bin/false".into(),
            agent_program: None,
            ..crate::config::Config::test_defaults()
        };
        let app = AppState::new(crate::store::Store::in_memory().await.unwrap(), config);
        app.lang.set(crate::lang::Lang::Es); // Spanish fixtures
        app.store
            .create_project(&delilah_core::Project {
                name: "web".into(),
                path: repo.clone(),
                kind: ProjectKind::Repo,
                workflow: "feature".into(),
                preview_command: None,
                settings,
            })
            .await
            .unwrap();
        (app, repo)
    }

    fn settings(setup: Option<&str>) -> delilah_core::ProjectSettings {
        let entry = |p: &str| delilah_core::SharedEntry {
            path: p.into(),
            mode: None,
        };
        delilah_core::ProjectSettings {
            shared: vec![entry(".env"), entry("README.md")],
            setup: setup.map(Into::into),
            ..Default::default()
        }
    }

    fn request(key: &str, in_checkout: bool) -> RunRequest {
        RunRequest {
            key: key.into(),
            title: "T".into(),
            project: "web".into(),
            notes: None,
            workflow: None,
            submodules: vec![],
            branch: None,
            prompt: None,
            free: true,
            in_checkout: Some(in_checkout),
            saving_mode: None,
            autopilot: None,
            from_session: None,
            chat_mode: None,
            ci_autofix: None,
        }
    }

    async fn status_of(app: &AppState, key: &str) -> TaskStatus {
        app.store
            .get_task(&TaskKey::new(key))
            .await
            .unwrap()
            .unwrap()
            .status
    }

    fn rules(always: &[&str], never: &[&str]) -> delilah_core::SubmoduleRules {
        let v = |l: &[&str]| l.iter().map(|s| s.to_string()).collect();
        delilah_core::SubmoduleRules {
            always: v(always),
            never: v(never),
        }
    }

    fn names(l: &[&str]) -> Vec<String> {
        l.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn always_is_added_after_the_requested_without_duplicates() {
        let available = names(&["a", "b", "c"]);
        let merged = effective_submodules(
            &names(&["c", "a"]),
            &rules(&["a", "b"], &[]),
            &available,
            crate::lang::Lang::En,
        )
        .unwrap();
        assert_eq!(merged, names(&["c", "a", "b"]));
        let merged =
            effective_submodules(&[], &rules(&["b"], &[]), &available, crate::lang::Lang::En)
                .unwrap();
        assert_eq!(merged, names(&["b"]));
    }

    #[test]
    fn a_requested_never_submodule_is_refused_naming_it() {
        let err = effective_submodules(
            &names(&["a", "c"]),
            &rules(&[], &["c"]),
            &names(&["a", "c"]),
            crate::lang::Lang::En,
        )
        .unwrap_err();
        assert_eq!(err.len(), 1);
        assert_eq!(err[0].field, "submodules[1]");
        assert!(err[0].message.contains("`c`"));
    }

    #[test]
    fn a_rule_naming_a_missing_submodule_points_at_the_settings() {
        let err = effective_submodules(
            &names(&["a"]),
            &rules(&["gone"], &[]),
            &names(&["a"]),
            crate::lang::Lang::Es,
        )
        .unwrap_err();
        assert_eq!(err[0].field, "settings.submodules.always[0]");
        assert!(err[0].message.contains("`gone`"));
        assert!(err[0].message.contains("ajustes del proyecto"));
    }

    #[tokio::test]
    async fn a_parent_run_with_a_never_or_stale_rule_is_refused_with_422() {
        let dir = std::env::temp_dir().join(format!("delilah-run-rules-{}", uuid::Uuid::new_v4()));
        let fixture = std::process::Command::new("bash")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../scripts/make-fixture-parent.sh"
            ))
            .arg(dir.join("env"))
            .output()
            .unwrap();
        assert!(fixture.status.success());
        let config = crate::config::Config {
            home: dir.join("home"),
            addr: crate::config::DEFAULT_ADDR.parse().unwrap(),
            allow_file_protocol: true,
            notify: false,
            ui_origin: None,
            claude_bin: "claude".into(),
            agent_program: None,
            ..crate::config::Config::test_defaults()
        };
        let app = AppState::new(crate::store::Store::in_memory().await.unwrap(), config);
        app.lang.set(crate::lang::Lang::Es); // Spanish fixtures
        let mut project = delilah_core::Project {
            name: "parent".into(),
            path: dir.join("env/parent"),
            kind: ProjectKind::Parent,
            workflow: "feature".into(),
            preview_command: None,
            settings: Default::default(),
        };
        project.settings.submodules = rules(&[], &["common-lib"]);
        app.store.create_project(&project).await.unwrap();
        let mut req = request("PROJ-1", false);
        req.project = "parent".into();
        req.submodules = names(&["svc-orders", "common-lib"]);
        let err = start_run(State(app.clone()), Json(req)).await.unwrap_err();
        assert_eq!(err.status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(err.fields[0].field, "submodules[1]");
        assert!(err.message.contains("common-lib"));

        project.settings.submodules = rules(&["svc-gone"], &[]);
        app.store
            .set_project_settings("parent", &project.settings)
            .await
            .unwrap();
        let mut req = request("PROJ-2", false);
        req.project = "parent".into();
        req.submodules = names(&["svc-orders"]);
        let err = start_run(State(app.clone()), Json(req)).await.unwrap_err();
        assert_eq!(err.status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(err.fields[0].field, "settings.submodules.always[0]");
        assert!(err.message.contains("svc-gone"));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[tokio::test]
    async fn a_worktree_with_a_setup_shares_then_waits_in_preparing_workspace() {
        let (app, _repo) = settings_app(settings(Some("sleep 30"))).await;
        let (code, Json(task)) = start_run(State(app.clone()), Json(request("LIBRE-1", false)))
            .await
            .ok()
            .expect("the run starts");
        assert_eq!(code, StatusCode::CREATED);
        assert_eq!(task.status, TaskStatus::PreparingWorkspace);
        let (_, ws) = app.store.workspace(&task.key).await.unwrap().unwrap();
        assert_eq!(
            std::fs::read_to_string(ws.path.join(".env")).unwrap(),
            "SECRET=1"
        );
        let log =
            std::fs::read_to_string(app.config.task_dir("LIBRE-1").join("workspace.log")).unwrap();
        assert_eq!(log.lines().count(), 2, "{log}");
        assert!(log.contains("README.md"), "{log}");
        // The setup runs, and the agent has not started.
        let live = app.sessions.of_task("LIBRE-1");
        assert_eq!(live.len(), 1);
        assert_eq!(live[0].kind, SessionKind::Setup);
        let _ = live[0].runtime.kill();
    }

    /// Waits for `key` to leave `preparing-workspace` (its setup ended).
    async fn wait_for_setup(app: &AppState, key: &str) {
        for _ in 0..200 {
            if status_of(app, key).await != TaskStatus::PreparingWorkspace {
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
        panic!("the setup of {key} did not end");
    }

    /// After the start: the first analysis finds no new folder, so the task keeps its
    /// title and stays without a branch.
    async fn assert_nothing_taken(app: &AppState, key: &str) {
        let key = TaskKey::new(key);
        let task = app.store.get_task(&key).await.unwrap().unwrap();
        crate::branching::recover(app, &task).await;
        let task = app.store.get_task(&key).await.unwrap().unwrap();
        assert_eq!(task.title, "T");
        assert_eq!(task.branch_error, None);
        let (_, ws) = app.store.workspace(&key).await.unwrap().unwrap();
        assert_eq!(ws.branch, "");
    }

    #[tokio::test]
    async fn a_shared_specs_folder_is_not_the_first_analysis() {
        let mut shared = settings(None);
        shared.shared = vec![delilah_core::SharedEntry {
            path: "specs".into(),
            mode: None,
        }];
        let (app, repo) = settings_app(shared).await;
        let old = repo.join("specs/001-old");
        std::fs::create_dir_all(&old).unwrap();
        std::fs::write(old.join("spec.md"), "# Feature Specification: Old\n").unwrap();
        let exclude = repo.join(".git/info/exclude");
        let text = std::fs::read_to_string(&exclude).unwrap();
        std::fs::write(&exclude, format!("{text}specs\n")).unwrap();
        let _ = start_run(State(app.clone()), Json(request("LIBRE-1", false)))
            .await
            .ok()
            .expect("the run starts");
        let at_start = app
            .store
            .spec_dirs_at_start(&TaskKey::new("LIBRE-1"))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(at_start, vec![".:001-old".to_string()]);
        assert_nothing_taken(&app, "LIBRE-1").await;
    }

    #[tokio::test]
    async fn a_setup_that_creates_spec_folders_is_not_the_first_analysis() {
        let setup = "mkdir -p specs/x && printf '# Feature Specification: X\\n' > specs/x/spec.md";
        let (app, _repo) = settings_app(settings(Some(setup))).await;
        let _ = start_run(State(app.clone()), Json(request("LIBRE-1", false)))
            .await
            .ok()
            .expect("the run starts");
        wait_for_setup(&app, "LIBRE-1").await;
        let at_start = app
            .store
            .spec_dirs_at_start(&TaskKey::new("LIBRE-1"))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(at_start, vec![".:x".to_string()]);
        assert_nothing_taken(&app, "LIBRE-1").await;
    }

    #[tokio::test]
    async fn a_worktree_without_a_setup_shares_and_launches() {
        let (app, _repo) = settings_app(settings(None)).await;
        let (code, Json(task)) = start_run(State(app.clone()), Json(request("LIBRE-1", false)))
            .await
            .ok()
            .expect("the run starts");
        assert_eq!(code, StatusCode::CREATED);
        assert_eq!(task.status, TaskStatus::Running);
        let (_, ws) = app.store.workspace(&task.key).await.unwrap().unwrap();
        assert!(ws.path.join(".env").exists());
        assert!(
            app.config
                .task_dir("LIBRE-1")
                .join("workspace.log")
                .exists()
        );
    }

    #[tokio::test]
    async fn the_checkout_never_runs_the_setup_nor_shares() {
        let (app, repo) = settings_app(settings(Some("touch ran-setup"))).await;
        let (code, Json(task)) = start_run(State(app.clone()), Json(request("LIBRE-1", true)))
            .await
            .map_err(|e| format!("{:?}", e.message))
            .expect("the run starts");
        assert_eq!(code, StatusCode::CREATED);
        assert_eq!(task.status, TaskStatus::Running);
        assert!(app.sessions.of_task("LIBRE-1").is_empty());
        assert!(
            !app.config
                .task_dir("LIBRE-1")
                .join("workspace.log")
                .exists()
        );
        assert!(!repo.join("ran-setup").exists());
        assert_eq!(status_of(&app, "LIBRE-1").await, TaskStatus::Running);
    }

    fn without_choice(key: &str) -> RunRequest {
        RunRequest {
            in_checkout: None,
            ..request(key, false)
        }
    }

    fn worktree_default() -> delilah_core::ProjectSettings {
        delilah_core::ProjectSettings {
            workspace: Some(delilah_core::WorkspaceMode::Worktree),
            ..settings(None)
        }
    }

    fn logged(app: &AppState, key: &str) -> bool {
        app.config.task_dir(key).join("workspace.log").exists()
    }

    /// Whether the task works in the project's own checkout (not a worktree).
    async fn in_checkout(app: &AppState, key: &str, repo: &std::path::Path) -> bool {
        let (_, ws) = app
            .store
            .workspace(&TaskKey::new(key))
            .await
            .unwrap()
            .unwrap();
        ws.path == repo
    }

    #[tokio::test]
    async fn no_choice_takes_the_projects_worktree_default() {
        let (app, repo) = settings_app(worktree_default()).await;
        let (_, Json(task)) = start_run(State(app.clone()), Json(without_choice("LIBRE-1")))
            .await
            .ok()
            .expect("the run starts");
        assert_eq!(task.status, TaskStatus::Running);
        assert!(logged(&app, "LIBRE-1"));
        assert!(!in_checkout(&app, "LIBRE-1", &repo).await);
    }

    #[tokio::test]
    async fn a_request_choice_wins_over_the_projects_default() {
        let (app, repo) = settings_app(worktree_default()).await;
        let (_, Json(task)) = start_run(State(app.clone()), Json(request("LIBRE-1", true)))
            .await
            .ok()
            .expect("the run starts");
        assert_eq!(task.status, TaskStatus::Running);
        assert!(!logged(&app, "LIBRE-1"));
        assert!(in_checkout(&app, "LIBRE-1", &repo).await);
    }

    #[tokio::test]
    async fn a_saved_backlog_choice_wins_over_the_projects_default() {
        let (app, repo) = settings_app(worktree_default()).await;
        app.store
            .create_task(&{
                let mut t = Task::new(TaskKey::new("LIBRE-1"), "T", "feature");
                t.project = Some("web".into());
                t.status = TaskStatus::Queued;
                t.in_checkout = Some(true);
                t
            })
            .await
            .unwrap();
        let (_, Json(task)) = start_run(State(app.clone()), Json(without_choice("LIBRE-1")))
            .await
            .ok()
            .expect("the run starts");
        assert_eq!(task.status, TaskStatus::Running);
        assert!(!logged(&app, "LIBRE-1"));
        assert!(in_checkout(&app, "LIBRE-1", &repo).await);
    }

    #[tokio::test]
    async fn no_choice_anywhere_works_in_the_checkout() {
        let (app, repo) = settings_app(settings(None)).await;
        let (_, Json(task)) = start_run(State(app.clone()), Json(without_choice("LIBRE-1")))
            .await
            .ok()
            .expect("the run starts");
        assert_eq!(task.status, TaskStatus::Running);
        assert!(!logged(&app, "LIBRE-1"));
        assert!(in_checkout(&app, "LIBRE-1", &repo).await);
    }

    #[tokio::test]
    async fn a_saved_backlog_worktree_choice_wins_over_the_default() {
        let (app, repo) = settings_app(settings(None)).await;
        app.store
            .create_task(&{
                let mut t = Task::new(TaskKey::new("LIBRE-1"), "T", "feature");
                t.project = Some("web".into());
                t.status = TaskStatus::Queued;
                t.in_checkout = Some(false);
                t
            })
            .await
            .unwrap();
        let (_, Json(task)) = start_run(State(app.clone()), Json(without_choice("LIBRE-1")))
            .await
            .ok()
            .expect("the run starts");
        assert_eq!(task.status, TaskStatus::Running);
        assert!(logged(&app, "LIBRE-1"));
        assert!(!in_checkout(&app, "LIBRE-1", &repo).await);
    }

    #[tokio::test]
    async fn the_project_setting_wins_over_the_default() {
        let (app, repo) = settings_app(worktree_default()).await;
        let (_, Json(task)) = start_run(State(app.clone()), Json(without_choice("LIBRE-1")))
            .await
            .ok()
            .expect("the run starts");
        assert_eq!(task.status, TaskStatus::Running);
        assert!(logged(&app, "LIBRE-1"));
        assert!(!in_checkout(&app, "LIBRE-1", &repo).await);
    }

    #[tokio::test]
    async fn a_checkout_setting_and_no_choice_start_in_the_checkout() {
        let (app, repo) = settings_app(delilah_core::ProjectSettings {
            workspace: Some(delilah_core::WorkspaceMode::Checkout),
            ..settings(None)
        })
        .await;
        let (_, Json(task)) = start_run(State(app.clone()), Json(without_choice("LIBRE-1")))
            .await
            .ok()
            .expect("the run starts");
        assert_eq!(task.status, TaskStatus::Running);
        assert!(!logged(&app, "LIBRE-1"));
        assert!(in_checkout(&app, "LIBRE-1", &repo).await);
    }
}
