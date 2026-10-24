//! `POST /api/v1/runs`: creates a task, builds its workspace and starts the agent.

use std::sync::Arc;
use std::time::Duration;

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use delilah_agents::claude;
use delilah_agents::pty::PtySession;
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

/// The `pr_notices.kind` of "the branch started from a base that could not be refreshed".
pub(crate) const KIND_BASE_STALE: &str = "base-stale";

/// Logs where each repo's task branch started (the ref and the commit), and warns in
/// the inbox for each repo whose fetch failed. Never fails the start.
async fn report_bases(app: &AppState, key: &TaskKey, project: &str, bases: &[RepoBase]) {
    let mut warned = false;
    for b in bases {
        tracing::info!(task = %key, repo = %b.repo, base = %b.refname, sha = %b.sha, fetch = ?b.fetch, "task branch base");
        let FetchOutcome::Failed(reason) = &b.fetch else {
            continue;
        };
        let name = if b.repo == "." { project } else { &b.repo };
        let sha7: String = b.sha.chars().take(7).collect();
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
            cause: "start".into(),
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

/// How a Claude session is launched, by what it is: a conversation never asks
/// (`DontAsk`) and gets the Delilah plugin and the idea prompt; a read-only one plans;
/// the plugin (which can create tasks) goes only to project sessions, never to a
/// task's agents.
fn launch_flags(
    session: &SessionRecord,
    config: &crate::config::Config,
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
        claude::Permission::Default
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
    /// Left out: what a backlog task chose, else the project's default workspace.
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
}

pub async fn start_run(
    State(app): State<AppState>,
    Json(mut req): Json<RunRequest>,
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
    // with none of them, a worktree.
    let in_checkout = req
        .in_checkout
        .or_else(|| existing.as_ref().and_then(|t| t.in_checkout))
        .or(project
            .settings
            .workspace
            .map(|m| m == delilah_core::WorkspaceMode::Checkout))
        .unwrap_or(false);
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
            if updated.autopilot.is_some() != autopilot {
                let value = autopilot.then(new_autopilot);
                app.store.set_autopilot(&key, value.as_ref()).await?;
            }
            updated.notes
        }
        None => {
            let mut task = Task::new(key.clone(), req.title.trim(), &workflow);
            task.project = Some(project.name.clone());
            task.saving_mode = saving_mode;
            task.autopilot = autopilot.then(new_autopilot);
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
    let deadline = Deadline::after(Duration::from_secs(30));
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
            // A backlog task stays in the backlog as it was, to be tried again.
            match existing {
                Some(task) => {
                    app.transition(
                        &key,
                        Event::StartAborted {
                            reason: reason.clone(),
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
                    if let Err(e) = app.store.update_backlog_task(&key, &undo).await {
                        tracing::warn!(task = %key, "cannot put it back as it was: {e}");
                    }
                    if let Err(e) = app.store.set_saving_mode(&key, task.saving_mode).await {
                        tracing::warn!(task = %key, "cannot put saving mode back: {e}");
                    }
                    if let Err(e) = app.store.set_autopilot(&key, task.autopilot.as_ref()).await {
                        tracing::warn!(task = %key, "cannot put autopilot back: {e}");
                    }
                }
                None => {
                    app.transition(
                        &key,
                        Event::Fail {
                            reason: reason.clone(),
                        },
                    )
                    .await?;
                }
            }
            return Err(ApiError::unprocessable(reason));
        }
    };
    app.store.save_workspace(&key, &project.path, &ws).await?;
    if let Some(previous) = &previous {
        app.store.set_checkout_previous(&key, previous).await?;
    }
    app.transition(&key, Event::WorkspaceReady).await?;
    report_bases(&app, &key, &project.name, &bases).await;

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
        app.transition(&key, Event::WorkspaceReady).await?;
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
#[derive(Debug, thiserror::Error)]
pub enum LaunchError {
    #[error(transparent)]
    Store(#[from] crate::store::StoreError),
    #[error("{0}")]
    Start(String),
    /// A kind of session that is not started this way.
    #[error("{0}")]
    Unsupported(&'static str),
}

impl From<LaunchError> for ApiError {
    fn from(e: LaunchError) -> Self {
        match e {
            LaunchError::Store(e) => e.into(),
            LaunchError::Start(message) => ApiError::internal(message),
            LaunchError::Unsupported(message) => ApiError::conflict(message),
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
    if let Some(mut session) = adopted {
        // The project's Claude becomes the task's: it goes on with its conversation,
        // now able to edit, on the task's branch.
        if let Some(live) = app.sessions.get(&session.id) {
            let _ = live.pty.kill();
            live.pty.wait().await;
        }
        app.store
            .adopt_session(
                &session.id,
                &key.to_string(),
                app.lang().pick("Principal", "Main"),
            )
            .await?;
        session.task = key.to_string();
        session.primary = true;
        session.read_only = false;
        session.label = app.lang().pick("Principal", "Main").into();
        launch_session_guarded(app, &session, &ws.path, None, true, guard)?;
        app.emit(crate::events::Topic::Sessions);
    } else {
        let (session, prompt) = first_session(app, key.as_str(), saving_mode, autopilot, prompt);
        app.store.create_session(&session).await?;
        launch_session_guarded(app, &session, &ws.path, Some(&prompt), false, guard)?;
    }
    Ok(())
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

/// `launch_session` for the handoff and autopilot code, which speaks `ApiError`.
pub fn launch_session_api(
    app: &AppState,
    session: &SessionRecord,
    workspace: &std::path::Path,
    prompt: Option<&str>,
    resume: bool,
) -> Result<(), ApiError> {
    launch_session(app, session, workspace, prompt, resume).map_err(Into::into)
}

/// Starts a session in a daemon-owned PTY: Claude (new, or resumed with `--resume`
/// on the same id) or the user's login shell, in the task's workspace.
pub fn launch_session(
    app: &AppState,
    session: &SessionRecord,
    workspace: &std::path::Path,
    prompt: Option<&str>,
    resume: bool,
) -> Result<(), LaunchError> {
    // Not told whether the task has its branch: any free task gets the guard, which
    // answers at once once the branch exists.
    let guard = crate::branching::is_free(&TaskKey::new(&session.task));
    launch_session_guarded(app, session, workspace, prompt, resume, guard)
}

/// [`launch_session`] knowing whether the session needs the branch guard hook: the
/// task is free and has no branch yet.
pub fn launch_session_guarded(
    app: &AppState,
    session: &SessionRecord,
    workspace: &std::path::Path,
    prompt: Option<&str>,
    resume: bool,
    guard: bool,
) -> Result<(), LaunchError> {
    // Nothing to resume if Claude never got a first message: start it afresh.
    let resume = resume && claude::has_conversation(workspace, &session.id);
    // No control character reaches the agent's command line.
    let prompt = prompt.map(crate::actions::sanitize_prompt);
    let prompt = prompt.as_deref();
    let task_dir = app.config.task_dir(&session.task);
    std::fs::create_dir_all(&task_dir)
        .map_err(|e| LaunchError::Start(format!("cannot create {}: {e}", task_dir.display())))?;
    let cmd = match session.kind {
        SessionKind::Claude => {
            let (permission, plugin_dir, append_system_prompt) = launch_flags(session, &app.config);
            let settings = claude::write_settings(
                &task_dir.join(format!("claude-{}.json", session.id)),
                &app.config.url(),
                &session.task,
                &session.id,
                permission,
                &[task_dir.as_path()],
                guard,
            )
            .map_err(|e| LaunchError::Start(format!("cannot write agent settings: {e}")))?;
            let remote_name = app.remote_control().then(|| remote_name(session));
            claude::command(&claude::Launch {
                workspace,
                session_id: &session.id,
                settings: &settings,
                prompt,
                resume,
                extra_dirs: &[task_dir.as_path()],
                permission,
                plugin_dir: plugin_dir.as_deref(),
                append_system_prompt,
                remote_control: remote_name.as_deref(),
                program: app.config.agent_program.as_deref(),
            })
        }
        SessionKind::Shell => shell_command(workspace),
        SessionKind::Setup => {
            return Err(LaunchError::Unsupported(
                "a setup session is started by the task and cannot be reopened",
            ));
        }
    };
    let log = task_dir.join(format!("{}-{}.log", session.kind.name(), session.id));
    let pty = PtySession::spawn(cmd, 120, 40, Some(log))
        .map_err(|e| LaunchError::Start(format!("cannot start the session: {e}")))?;
    let live = app.sessions.insert(
        &session.id,
        &session.task,
        session.kind,
        session.primary,
        Arc::clone(&pty),
    );
    live.set_conversation(session.conversation);
    let remote_control = session.kind == SessionKind::Claude && app.remote_control();
    tracing::info!(task = %session.task, session = %session.id, kind = session.kind.name(), resume, remote_control, "session started");

    if session.kind == SessionKind::Claude {
        tokio::spawn(accept_trust_dialog(
            Arc::clone(&pty),
            Arc::clone(&live.started),
        ));
    }
    let sessions = Arc::clone(&app.sessions);
    let id = session.id.clone();
    tokio::spawn(async move {
        pty.wait().await;
        sessions.remove_if_same(&id, &pty);
        tracing::info!(session = %id, "session exited");
    });
    Ok(())
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
        let ws = delilah_git::create_workspace(&project.path, &spec, &opts).unwrap();
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
        let (p, dir, prompt) = launch_flags(&conv, &config);
        assert_eq!(p, claude::Permission::DontAsk);
        assert_eq!(dir, Some(config.plugin_dir()));
        assert_eq!(prompt, Some(IDEA_PROMPT));

        let mut ro = session("@web", "Claude 1");
        ro.read_only = true;
        let (p, dir, prompt) = launch_flags(&ro, &config);
        assert_eq!(p, claude::Permission::Plan);
        assert!(dir.is_some());
        assert_eq!(prompt, None);

        let (p, dir, prompt) = launch_flags(&session("PROJ-1", "Claude 1"), &config);
        assert_eq!(p, claude::Permission::Default);
        assert_eq!((dir, prompt), (None, None));

        // An extra session of a task is no different: it never gets the plugin.
        let mut extra = session("PROJ-1", "Claude 2");
        extra.primary = false;
        let (p, dir, prompt) = launch_flags(&extra, &config);
        assert_eq!(p, claude::Permission::Default);
        assert_eq!((dir, prompt), (None, None));
        extra.read_only = true;
        let (p, dir, prompt) = launch_flags(&extra, &config);
        assert_eq!(p, claude::Permission::Plan);
        assert_eq!((dir, prompt), (None, None));
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
        assert_ne!(
            sh(&repo, &["rev-parse", "HEAD"]),
            remote_head,
            "the project folder is untouched"
        );
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
        assert_eq!(
            (n.kind.as_str(), n.repo.as_str(), n.cause.as_str()),
            ("base-stale", ".", "start")
        );
        assert!(n.text.contains("stale"), "{}", n.text);
        assert!(n.text.contains("origin/main @ "), "{}", n.text);
        assert!(
            n.text.contains("/nonexistent") || n.text.contains("fetch"),
            "{}",
            n.text
        );
    }

    #[tokio::test]
    async fn no_remote_or_an_existing_branch_leave_no_notice() {
        let app = repo_project(None).await;
        start_stale(&app, "PROJ-7", false).await;
        assert!(app.store.open_notices().await.unwrap().is_empty());

        // Checkout mode with the branch already there: it is not re-based.
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
        assert!(app.store.open_notices().await.unwrap().is_empty());
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
            let _ = live.pty.kill();
        }
        assert!(
            args.contains(&format!("{})", path.display())),
            "the prompt does not point to notes.md: {args}"
        );
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
                    serde_json::json!({ "key": key, "title": "Idea", "project": "web", "free": free })
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
                let _ = live.pty.kill();
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
        let _ = live[0].pty.kill();
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
    async fn no_workspace_setting_and_no_choice_start_in_a_worktree() {
        let (app, repo) = settings_app(settings(None)).await;
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
