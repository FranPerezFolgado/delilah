//! `POST /api/v1/runs`: creates a task, builds its workspace and starts the agent.

use std::sync::Arc;
use std::time::Duration;

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use delilah_agents::claude;
use delilah_agents::pty::PtySession;
use delilah_agents::runtime::{self, RuntimeKind};
use delilah_core::{Event, ProjectKind, Step, Task, TaskKey, TaskStatus};
use delilah_git::{GitOptions, WorkspaceSpec};
use serde::Deserialize;
use tokio::sync::broadcast::error::RecvError;

use crate::api::ApiError;
use crate::state::AppState;
use crate::store::BacklogChange;
use crate::store_sessions::{SessionKind, SessionRecord, project_of};

/// How long a new session may show the folder-trust dialog before we stop watching.
const TRUST_WATCH: Duration = Duration::from_secs(60);

/// Added to the system prompt of an idea conversation.
const IDEA_PROMPT: &str = "This is a Delilah idea conversation, shown in a chat view, not a terminal. Follow the task-from-idea skill from the first message. Ask with AskUserQuestion. Never use ExitPlanMode.";

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
    /// Defaults to a name Claude proposes from the title, as `feature/…`; always
    /// normalised and made unique.
    pub branch: Option<String>,
    /// Defaults to `/feature <key>` for the `feature` workflow (`/feature <title>`
    /// for a free task).
    pub prompt: Option<String>,
    /// A task without a Jira ticket: `key` may be empty, Delilah names it `LIBRE-<n>`.
    #[serde(default)]
    pub free: bool,
    /// Work in the project's own checkout (a branch there) instead of a worktree.
    /// Left out: what a backlog task chose, else a worktree.
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
    let in_checkout = req
        .in_checkout
        .or_else(|| existing.as_ref().and_then(|t| t.in_checkout))
        .unwrap_or(false);
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
    let branch = crate::branch_name::for_start(
        &app,
        &project,
        &req.key,
        &req.title,
        notes.as_deref(),
        req.free,
        &req.submodules,
        req.branch.as_deref(),
    )
    .await?;

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
            updated.notes
        }
        None => {
            let mut task = Task::new(key.clone(), req.title.trim(), &workflow);
            task.project = Some(project.name.clone());
            task.saving_mode = saving_mode;
            task.autopilot = autopilot.then(new_autopilot);
            task.chat_mode = req.chat_mode.unwrap_or(false);
            task.notes = req.notes.clone().filter(|n| !n.trim().is_empty());
            app.store.create_task(&task).await?;
            task.notes
        }
    };
    app.transition(&key, Event::Start).await?;

    let spec = WorkspaceSpec {
        path: app.config.workspaces_dir().join(&req.key),
        branch,
        kind: project.kind,
        submodules: req.submodules.clone(),
    };
    let opts = GitOptions {
        allow_file_protocol: app.config.allow_file_protocol,
    };
    let root = project.path.clone();
    let created = tokio::task::spawn_blocking(move || {
        if in_checkout {
            delilah_git::use_checkout(&root, &spec, &opts).map(|(ws, prev)| (ws, Some(prev)))
        } else {
            delilah_git::create_workspace(&root, &spec, &opts).map(|ws| (ws, None))
        }
    })
    .await
    .map_err(|e| ApiError::internal(e.to_string()))?;
    let (ws, previous) = match created {
        Ok(created) => created,
        Err(e) => {
            let reason = format!("workspace: {e}");
            abort_start(&app, &key, existing, &reason).await?;
            return Err(ApiError::unprocessable(reason));
        }
    };
    app.store.save_workspace(&key, &project.path, &ws).await?;
    if let Some(previous) = &previous {
        app.store.set_checkout_previous(&key, previous).await?;
    }
    // A chat-mode agent can fail to come up: its task stays in the starting state until it
    // has, so the start can be undone.
    let chat = adopted.is_none() && app.store.get_task(&key).await?.is_some_and(|t| t.chat_mode);
    if !chat {
        app.transition(&key, Event::WorkspaceReady).await?;
    }

    // From the default workflow, hand the agent the ticket itself: it may not be able
    // to read Jira on its own. Kept outside the repo, so the workspace stays clean.
    let issue = match req.free {
        true => None,
        false => crate::jira::fetch_issue(&app, &req.key).await,
    };
    let prompt = match (&req.prompt, issue) {
        (None, Some(issue)) => {
            let path = app.config.task_dir(&req.key).join("ticket.md");
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
    let prompt = point_to_notes(&app, &key, prompt, notes.as_deref());

    if let Some(mut session) = adopted {
        // The project's Claude becomes the task's: it goes on with its conversation,
        // now able to edit, on the task's branch.
        if let Some(live) = app.sessions.get(&session.id) {
            let _ = live.runtime.kill();
            live.runtime.wait().await;
        }
        app.store
            .adopt_session(&session.id, &req.key, app.lang().pick("Principal", "Main"))
            .await?;
        session.task = req.key.clone();
        session.primary = true;
        session.read_only = false;
        session.label = app.lang().pick("Principal", "Main").into();
        launch_session(&app, &session, &ws.path, None, true).await?;
        app.emit(crate::events::Topic::Sessions);
    } else {
        let (session, prompt) = first_session(&app, &req.key, saving_mode, autopilot, prompt);
        app.store.create_session(&session).await?;
        if let Err(e) = launch_session(&app, &session, &ws.path, Some(&prompt), false).await {
            if chat {
                undo_chat_start(&app, &key, &session, existing, &e.message).await;
            }
            return Err(e);
        }
        if chat {
            app.transition(&key, Event::WorkspaceReady).await?;
        }
    }

    let task = app
        .store
        .get_task(&key)
        .await?
        .expect("task was just created");
    Ok((StatusCode::CREATED, Json(task)))
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
async fn abort_start(
    app: &AppState,
    key: &TaskKey,
    existing: Option<Task>,
    reason: &str,
) -> Result<(), ApiError> {
    match existing {
        Some(task) => {
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
        None => {
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

/// A chat-mode agent that never came up: undoes the start so the task can be started
/// again (the app offers the terminal). The session and the workspace go; the task goes
/// back to where [`abort_start`] puts it.
async fn undo_chat_start(
    app: &AppState,
    key: &TaskKey,
    session: &SessionRecord,
    existing: Option<Task>,
    reason: &str,
) {
    if let Err(e) = sqlx::query("DELETE FROM sessions WHERE id = ?")
        .bind(&session.id)
        .execute(&app.store.pool)
        .await
    {
        tracing::warn!(task = %key, "cannot forget the session: {e}");
    }
    if let Err(e) = crate::archive::release_workspace(app, key, true).await {
        tracing::warn!(task = %key, "cannot release the workspace: {}", e.message());
    }
    if let Err(e) = app.store.delete_workspace(key).await {
        tracing::warn!(task = %key, "cannot forget the workspace: {e}");
    }
    if let Err(e) = abort_start(app, key, existing, reason).await {
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

/// How long a chat-mode agent has to say hello (its `Init` line) before the start fails.
const INIT_TIMEOUT: Duration = Duration::from_secs(30);

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
    let chat = app
        .store
        .get_task(&TaskKey::new(&session.task))
        .await?
        .is_some_and(|t| t.chat_mode);
    Ok(match chat {
        true => RuntimeKind::Stream,
        false => RuntimeKind::Pty,
    })
}

/// Starts a session in a daemon-owned PTY (or, for a chat-mode task, a stream-json
/// process): Claude (new, or resumed with `--resume` on the same id) or the user's
/// login shell, in the task's workspace. A stream session is started once the agent
/// has said hello, and gets the prompt then. Boxed, so the handoffs can take it (and
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
        INIT_TIMEOUT,
    ))
}

pub(crate) async fn launch_session_with(
    app: &AppState,
    session: &SessionRecord,
    workspace: &std::path::Path,
    prompt: Option<&str>,
    resume: bool,
    init_timeout: Duration,
) -> Result<(), ApiError> {
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
            let (permission, plugin_dir, append_system_prompt) = launch_flags(session, &app.config);
            let settings = claude::write_settings(
                &task_dir.join(format!("claude-{}.json", session.id)),
                &app.config.url(),
                &session.task,
                &session.id,
                permission,
                &[task_dir.as_path()],
            )
            .map_err(|e| ApiError::internal(format!("cannot write agent settings: {e}")))?;
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
    // wait on it (spike d1, decision 49).
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
    tokio::spawn(async move {
        watched.wait().await;
        sessions.remove_if_same(&id, &watched);
        tracing::info!(session = %id, "session exited");
    });

    if let (Some(events), Some(stream)) = (events, runtime.as_stream()) {
        let started = crate::stream_events::spawn_pump(app, &session.id, &session.task, events);
        let up = matches!(
            tokio::time::timeout(init_timeout, started).await,
            Ok(Ok(crate::stream_events::Started::Init))
        );
        if !up {
            let _ = runtime.kill();
            runtime.wait().await;
            let tail = start_error_tail(&stream.stderr_tail());
            let tail = match tail.trim() {
                "" => "claude printed nothing before it stopped",
                tail => tail,
            };
            return Err(ApiError::unprocessable(format!(
                "chat mode could not start: {tail}"
            )));
        }
        if let Some(prompt) = prompt {
            runtime
                .send_prompt(prompt)
                .await
                .map_err(|e| ApiError::internal(format!("cannot send the first message: {e}")))?;
        }
    }
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

#[cfg(test)]
mod tests {
    use super::*;

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
            claude_projects: crate::config::Config::scratch_projects(),
            claude_bin: "claude".into(),
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
            claude_projects: crate::config::Config::scratch_projects(),
        };
        let app = AppState::new(crate::store::Store::in_memory().await.unwrap(), config);
        // A folder that isn't there: any workspace for it fails, and no agent starts.
        app.store
            .create_project(&delilah_core::Project {
                name: "web".into(),
                path: app.config.home.join("missing"),
                kind: ProjectKind::Repo,
                workflow: "feature".into(),
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

    #[tokio::test]
    async fn a_free_task_is_named_by_claude() {
        let (app, _) = repo_app(NAMES_IT).await;
        let mut req = run("");
        req.title = "Nombres de rama".into();
        req.free = true;
        assert_eq!(started(&app, req).await, "feature/task-branch-names");
        let (_, ws) = app
            .store
            .workspace(&TaskKey::new("LIBRE-1"))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            git(&ws.path, &["branch", "--show-current"]),
            "feature/task-branch-names"
        );
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
    async fn without_claude_an_unusable_free_title_gives_the_key() {
        let (app, _) = repo_app("exit 1").await;
        let mut req = run("");
        req.title = "!!".into();
        req.free = true;
        assert_eq!(started(&app, req).await, "feature/libre-1");
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
            claude_projects: crate::config::Config::scratch_projects(),
            claude_bin: "claude".into(),
        };
        let app = AppState::new(crate::store::Store::in_memory().await.unwrap(), config);
        app.store
            .create_project(&delilah_core::Project {
                name: "web".into(),
                path: repo,
                kind: ProjectKind::Repo,
                workflow: "feature".into(),
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
        let mut task = Task::new(TaskKey::new("PROJ-1"), "t", "feature");
        task.project = Some("web".into());
        task.chat_mode = chat_mode;
        app.store.create_task(&task).await.unwrap();
        (app, ws)
    }

    /// Says hello like a stream agent, saves its arguments and its first message.
    fn hello_script(home_args: &str) -> String {
        format!(
            "printf '%s\\n' \"$@\" > '{home_args}/args.txt'\n\
             echo '{{\"type\":\"system\",\"subtype\":\"init\",\"session_id\":\"sess-1\"}}'\n\
             read line; printf '%s' \"$line\" > '{home_args}/stdin.txt'\n\
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
    async fn an_agent_that_never_says_hello_times_out() {
        let (app, ws) = chat_app("sleep 5", true).await;
        let err = launch_session_with(
            &app,
            &session("PROJ-1", "Main"),
            &ws,
            None,
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

    #[test]
    fn remote_names_show_the_task_or_project_and_the_label() {
        assert_eq!(
            remote_name(&session("PROJ-1", "Principal")),
            "PROJ-1 · Principal"
        );
        assert_eq!(remote_name(&session("@delilah", "Main")), "delilah · Main");
    }
}
