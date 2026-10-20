//! `POST /api/v1/runs`: creates a task, builds its workspace and starts the agent.

use std::sync::Arc;
use std::time::Duration;

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use delilah_agents::claude;
use delilah_agents::pty::PtySession;
use delilah_core::{Event, ProjectKind, Task, TaskKey};
use delilah_git::{GitOptions, WorkspaceSpec};
use serde::Deserialize;
use tokio::sync::broadcast::error::RecvError;

use crate::api::ApiError;
use crate::state::AppState;
use crate::store_sessions::{SessionKind, SessionRecord};

/// How long a new session may show the folder-trust dialog before we stop watching.
const TRUST_WATCH: Duration = Duration::from_secs(60);

#[derive(Debug, Deserialize)]
pub struct RunRequest {
    pub key: String,
    pub title: String,
    /// Name of a registered project.
    pub project: String,
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
    #[serde(default)]
    pub in_checkout: bool,
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
            req.in_checkout = true;
            Some(s)
        }
        None => None,
    };
    if req.free && req.key.trim().is_empty() {
        req.key = next_free_key(&app).await?;
    }
    if req.free && req.title.trim().is_empty() {
        return Err(ApiError::bad_request("say what the free task is about"));
    }
    let key = TaskKey::parse(&req.key).map_err(|e| ApiError::bad_request(e.to_string()))?;
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

    if req.in_checkout {
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

    let mut task = Task::new(key.clone(), &req.title, &workflow);
    task.project = Some(project.name.clone());
    app.store.create_task(&task).await?;
    app.transition(&key, Event::Start).await?;

    let spec = WorkspaceSpec {
        path: app.config.workspaces_dir().join(&req.key),
        branch: req
            .branch
            .clone()
            .unwrap_or_else(|| format!("feature/{}", req.key)),
        kind: project.kind,
        submodules: req.submodules.clone(),
    };
    let opts = GitOptions {
        allow_file_protocol: app.config.allow_file_protocol,
    };
    let root = project.path.clone();
    let in_checkout = req.in_checkout;
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
            app.transition(
                &key,
                Event::Fail {
                    reason: reason.clone(),
                },
            )
            .await?;
            return Err(ApiError::unprocessable(reason));
        }
    };
    app.store.save_workspace(&key, &project.path, &ws).await?;
    if let Some(previous) = &previous {
        app.store.set_checkout_previous(&key, previous).await?;
    }
    app.transition(&key, Event::WorkspaceReady).await?;

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

    if let Some(mut session) = adopted {
        // The project's Claude becomes the task's: it goes on with its conversation,
        // now able to edit, on the task's branch.
        if let Some(live) = app.sessions.get(&session.id) {
            let _ = live.pty.kill();
            live.pty.wait().await;
        }
        app.store
            .adopt_session(&session.id, &req.key, app.lang().pick("Principal", "Main"))
            .await?;
        session.task = req.key.clone();
        session.primary = true;
        session.read_only = false;
        session.label = app.lang().pick("Principal", "Main").into();
        launch_session(&app, &session, &ws.path, None, true)?;
        app.emit(crate::events::Topic::Sessions);
    } else {
        let session = SessionRecord {
            id: uuid::Uuid::new_v4().to_string(),
            task: req.key.clone(),
            kind: SessionKind::Claude,
            label: app.lang().pick("Principal", "Main").into(),
            primary: true,
            closed: false,
            read_only: false,
        };
        app.store.create_session(&session).await?;
        launch_session(&app, &session, &ws.path, Some(&prompt), false)?;
    }

    let task = app
        .store
        .get_task(&key)
        .await?
        .expect("task was just created");
    Ok((StatusCode::CREATED, Json(task)))
}

/// Starts a session in a daemon-owned PTY: Claude (new, or resumed with `--resume`
/// on the same id) or the user's login shell, in the task's workspace.
pub fn launch_session(
    app: &AppState,
    session: &SessionRecord,
    workspace: &std::path::Path,
    prompt: Option<&str>,
    resume: bool,
) -> Result<(), ApiError> {
    // Nothing to resume if Claude never got a first message: start it afresh.
    let resume = resume && claude::has_conversation(workspace, &session.id);
    let task_dir = app.config.task_dir(&session.task);
    std::fs::create_dir_all(&task_dir)
        .map_err(|e| ApiError::internal(format!("cannot create {}: {e}", task_dir.display())))?;
    let cmd = match session.kind {
        SessionKind::Claude => {
            let settings = claude::write_settings(
                &task_dir.join(format!("claude-{}.json", session.id)),
                &app.config.url(),
                &session.task,
                &session.id,
            )
            .map_err(|e| ApiError::internal(format!("cannot write agent settings: {e}")))?;
            claude::command(&claude::Launch {
                workspace,
                session_id: &session.id,
                settings: &settings,
                prompt,
                resume,
                extra_dirs: &[task_dir.as_path()],
                read_only: session.read_only && session.kind == SessionKind::Claude,
            })
        }
        SessionKind::Shell => shell_command(workspace),
    };
    let log = task_dir.join(format!("{}-{}.log", session.kind.name(), session.id));
    let pty = PtySession::spawn(cmd, 120, 40, Some(log))
        .map_err(|e| ApiError::internal(format!("cannot start the session: {e}")))?;
    let live = app.sessions.insert(
        &session.id,
        &session.task,
        session.kind,
        session.primary,
        Arc::clone(&pty),
    );
    tracing::info!(task = %session.task, session = %session.id, kind = session.kind.name(), resume, "session started");

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

/// The next free task's key: `LIBRE-1`, `LIBRE-2`…
async fn next_free_key(app: &AppState) -> Result<String, ApiError> {
    let n = app
        .store
        .list_tasks()
        .await?
        .iter()
        .filter_map(|t| t.key.as_str().strip_prefix("LIBRE-")?.parse::<u32>().ok())
        .max()
        .unwrap_or(0);
    Ok(format!("LIBRE-{}", n + 1))
}
