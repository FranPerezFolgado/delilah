//! `POST /api/v1/runs`: creates a task, builds its workspace and starts the agent.

use std::sync::Arc;
use std::time::Duration;

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use delilah_agents::claude;
use delilah_agents::pty::PtySession;
use delilah_core::{Event, ProjectKind, Task, TaskKey, TaskStatus};
use delilah_git::{GitOptions, WorkspaceSpec};
use serde::Deserialize;
use tokio::sync::broadcast::error::RecvError;

use crate::api::ApiError;
use crate::state::AppState;
use crate::store::BacklogChange;
use crate::store_sessions::{SessionKind, SessionRecord, project_of};

/// How long a new session may show the folder-trust dialog before we stop watching.
const TRUST_WATCH: Duration = Duration::from_secs(60);

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
    /// Defaults to `feature/<key>`.
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
    let in_checkout = req
        .in_checkout
        .or_else(|| existing.as_ref().and_then(|t| t.in_checkout))
        .unwrap_or(false);
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
            app.store.update_backlog_task(&key, &change).await?.notes
        }
        None => {
            let mut task = Task::new(key.clone(), req.title.trim(), &workflow);
            task.project = Some(project.name.clone());
            task.notes = req.notes.clone().filter(|n| !n.trim().is_empty());
            app.store.create_task(&task).await?;
            task.notes
        }
    };
    app.transition(&key, Event::Start).await?;

    let spec = workspace_spec(
        app.config.workspaces_dir().join(&req.key),
        &project,
        req.branch.as_deref(),
        &key,
        &req.title,
        &req.submodules,
    );
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
    let launch = AgentLaunch {
        free: req.free,
        prompt,
        custom_prompt: req.prompt.is_some(),
        notes,
        adopted,
    };

    // A new worktree gets the shared entries and then the setup; the project's own
    // checkout gets neither.
    let setup = match previous {
        Some(_) => None,
        None => {
            let (checkout, worktree) = (project.path.clone(), ws.path.clone());
            let shared = project.settings.shared.clone();
            let notes = tokio::task::spawn_blocking(move || {
                delilah_git::share_entries(&checkout, &worktree, &shared)
            })
            .await
            .map_err(|e| ApiError::internal(e.to_string()))?;
            log_share_notes(&app, &key, &notes);
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
    #[cfg(test)]
    {
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
            })
            .await?;
        Ok(())
    }
    #[cfg(not(test))]
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
}

impl From<&AgentLaunch> for SavedLaunch {
    fn from(l: &AgentLaunch) -> Self {
        Self {
            free: l.free,
            prompt: l.prompt.clone(),
            custom_prompt: l.custom_prompt,
            notes: l.notes.clone(),
            adopted: l.adopted.as_ref().map(|s| s.id.clone()),
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
    })
}

/// Drops the saved launch (the agent started, or the task was archived).
pub(crate) fn forget_launch(app: &AppState, key: &TaskKey) {
    let _ = std::fs::remove_file(app.config.task_dir(key.as_str()).join(LAUNCH_FILE));
}

/// Everything after the workspace is ready: the ticket and notes the agent is
/// pointed at, then its primary session (an adopted one or a new one). Owns what it
/// needs, so it can run from a background task as well as from the request.
#[cfg_attr(test, allow(dead_code))]
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
        launch_session(app, &session, &ws.path, None, true)?;
        app.emit(crate::events::Topic::Sessions);
    } else {
        let session = SessionRecord {
            id: uuid::Uuid::new_v4().to_string(),
            task: key.to_string(),
            kind: SessionKind::Claude,
            label: app.lang().pick("Principal", "Main").into(),
            primary: true,
            closed: false,
            read_only: false,
        };
        app.store.create_session(&session).await?;
        launch_session(app, &session, &ws.path, Some(&prompt), false)?;
    }
    Ok(())
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
    // Nothing to resume if Claude never got a first message: start it afresh.
    let resume = resume && claude::has_conversation(workspace, &session.id);
    let task_dir = app.config.task_dir(&session.task);
    std::fs::create_dir_all(&task_dir)
        .map_err(|e| LaunchError::Start(format!("cannot create {}: {e}", task_dir.display())))?;
    let cmd = match session.kind {
        SessionKind::Claude => {
            let settings = claude::write_settings(
                &task_dir.join(format!("claude-{}.json", session.id)),
                &app.config.url(),
                &session.task,
                &session.id,
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
                read_only: session.read_only && session.kind == SessionKind::Claude,
                remote_control: remote_name.as_deref(),
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

/// The branch a task works on: the one asked for, else the project's branch template.
fn default_branch(
    explicit: Option<&str>,
    settings: &delilah_core::ProjectSettings,
    key: &TaskKey,
    title: &str,
) -> String {
    match explicit.map(str::trim).filter(|b| !b.is_empty()) {
        Some(branch) => branch.to_string(),
        None => delilah_core::branch_name(&settings.branch, key.as_str(), title.trim()),
    }
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

/// What to build for a task: its branch (see [`default_branch`]) in every repo.
fn workspace_spec(
    path: std::path::PathBuf,
    project: &delilah_core::Project,
    explicit_branch: Option<&str>,
    key: &TaskKey,
    title: &str,
    submodules: &[String],
) -> WorkspaceSpec {
    WorkspaceSpec {
        path,
        branch: default_branch(explicit_branch, &project.settings, key, title),
        kind: project.kind,
        submodules: submodules.to_vec(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn templated(template: &str) -> delilah_core::ProjectSettings {
        delilah_core::ProjectSettings {
            branch: template.into(),
            ..Default::default()
        }
    }

    #[test]
    fn branch_follows_the_template() {
        let key = TaskKey::new("PROJ-7");
        let s = templated("feature/{ticket}-{slug}");
        assert_eq!(
            default_branch(None, &s, &key, "Fix the Login bug"),
            "feature/PROJ-7-fix-the-login-bug"
        );
    }

    #[test]
    fn explicit_branch_wins_over_the_template() {
        let key = TaskKey::new("PROJ-7");
        let s = templated("feature/{ticket}-{slug}");
        assert_eq!(default_branch(Some("hotfix/x"), &s, &key, "t"), "hotfix/x");
        assert_eq!(
            default_branch(Some("  "), &s, &key, "Some title"),
            "feature/PROJ-7-some-title"
        );
    }

    #[test]
    fn no_settings_means_feature_key() {
        let key = TaskKey::new("PROJ-7");
        let s = delilah_core::ProjectSettings::default();
        assert_eq!(default_branch(None, &s, &key, "anything"), "feature/PROJ-7");
    }

    #[test]
    fn every_selected_submodule_is_on_the_templated_branch() {
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
            settings: templated("feature/{ticket}-{slug}"),
        };
        let subs = vec!["svc-orders".to_string(), "svc-payments".to_string()];
        let spec = workspace_spec(
            dir.join("workspaces/PROJ-9"),
            &project,
            None,
            &TaskKey::new("PROJ-9"),
            "Add Retry",
            &subs,
        );
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
        }
    }

    async fn app() -> AppState {
        let config = crate::config::Config {
            home: std::env::temp_dir().join(format!("delilah-run-{}", uuid::Uuid::new_v4())),
            addr: crate::config::DEFAULT_ADDR.parse().unwrap(),
            allow_file_protocol: false,
            notify: false,
            ui_origin: None,
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

    #[test]
    fn remote_names_show_the_task_or_project_and_the_label() {
        assert_eq!(
            remote_name(&session("PROJ-1", "Principal")),
            "PROJ-1 · Principal"
        );
        assert_eq!(remote_name(&session("@delilah", "Main")), "delilah · Main");
    }

    /// A repo project with one commit and an untracked `.env`, with these settings.
    async fn repo_app(settings: delilah_core::ProjectSettings) -> (AppState, std::path::PathBuf) {
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
        };
        let app = AppState::new(crate::store::Store::in_memory().await.unwrap(), config);
        app.store
            .create_project(&delilah_core::Project {
                name: "web".into(),
                path: repo.clone(),
                kind: ProjectKind::Repo,
                workflow: "feature".into(),
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
        };
        let app = AppState::new(crate::store::Store::in_memory().await.unwrap(), config);
        let mut project = delilah_core::Project {
            name: "parent".into(),
            path: dir.join("env/parent"),
            kind: ProjectKind::Parent,
            workflow: "feature".into(),
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
        let (app, _repo) = repo_app(settings(Some("sleep 30"))).await;
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

    #[tokio::test]
    async fn a_worktree_without_a_setup_shares_and_launches() {
        let (app, _repo) = repo_app(settings(None)).await;
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
        let (app, repo) = repo_app(settings(Some("touch ran-setup"))).await;
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
}
