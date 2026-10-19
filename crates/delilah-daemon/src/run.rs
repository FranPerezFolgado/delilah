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
    /// Defaults to `/feature <key>` for the `feature` workflow.
    pub prompt: Option<String>,
}

pub async fn start_run(
    State(app): State<AppState>,
    Json(req): Json<RunRequest>,
) -> Result<(StatusCode, Json<Task>), ApiError> {
    if req.key.is_empty()
        || req.key.len() > 64
        || !req
            .key
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err(ApiError::bad_request(format!(
            "`{}` is not a valid ticket key (letters, digits, - and _)",
            req.key
        )));
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
        .unwrap_or_else(|| project.workflow.clone());
    let prompt = match (&req.prompt, workflow.as_str()) {
        (Some(prompt), _) => prompt.clone(),
        (None, "feature") => format!("/feature {}", req.key),
        (None, other) => {
            return Err(ApiError::bad_request(format!(
                "workflow `{other}` needs a --prompt"
            )));
        }
    };

    let key = TaskKey::new(&req.key);
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
    let created =
        tokio::task::spawn_blocking(move || delilah_git::create_workspace(&root, &spec, &opts))
            .await
            .map_err(|e| ApiError::internal(e.to_string()))?;
    let ws = match created {
        Ok(ws) => ws,
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
    app.transition(&key, Event::WorkspaceReady).await?;

    // From the default workflow, hand the agent the ticket itself: it may not be able
    // to read Jira on its own. Kept outside the repo, so the workspace stays clean.
    let prompt = match (&req.prompt, crate::jira::fetch_issue(&app, &req.key).await) {
        (None, Some(issue)) => {
            let path = app.config.task_dir(&req.key).join("ticket.md");
            match crate::jira::write_ticket(&path, &issue) {
                Ok(()) => format!("{prompt} (el ticket de Jira está en {})", path.display()),
                Err(e) => {
                    tracing::warn!(task = %key, "cannot write the ticket: {e}");
                    prompt
                }
            }
        }
        _ => prompt,
    };

    let session = SessionRecord {
        id: uuid::Uuid::new_v4().to_string(),
        task: req.key.clone(),
        kind: SessionKind::Claude,
        label: "Principal".into(),
        primary: true,
        closed: false,
    };
    app.store.create_session(&session).await?;
    launch_session(&app, &session, &ws.path, Some(&prompt), false)?;

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
                workspace,
            )
            .map_err(|e| ApiError::internal(format!("cannot write agent settings: {e}")))?;
            claude::command(&claude::Launch {
                workspace,
                session_id: &session.id,
                settings: &settings,
                prompt,
                resume,
                extra_dirs: &[task_dir.as_path()],
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

/// A new workspace path shows Claude's folder-trust dialog once: accept it with Enter.
/// Stops as soon as the session starts, so it never presses Enter on the user's input.
async fn accept_trust_dialog(pty: Arc<PtySession>, started: Arc<tokio::sync::Notify>) {
    let (mut screen, mut rx) = pty.attach();
    let deadline = tokio::time::sleep(TRUST_WATCH);
    tokio::pin!(deadline);
    loop {
        if claude::shows_trust_dialog(&screen) {
            let _ = pty.write(b"\r");
            tracing::info!("accepted the folder-trust dialog");
            return;
        }
        tokio::select! {
            chunk = rx.recv() => match chunk {
                Ok(chunk) => screen.extend_from_slice(&chunk),
                Err(RecvError::Lagged(_)) => continue,
                Err(RecvError::Closed) => return,
            },
            _ = started.notified() => return,
            _ = &mut deadline => return,
        }
    }
}
