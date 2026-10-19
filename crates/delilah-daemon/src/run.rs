//! `POST /api/v1/runs`: creates a task, builds its workspace and starts the agent.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use chrono::Utc;
use delilah_agents::claude;
use delilah_agents::pty::PtySession;
use delilah_core::{Event, Task, TaskKey};
use delilah_git::{GitOptions, WorkspaceSpec};
use serde::Deserialize;
use tokio::sync::broadcast::error::RecvError;

use crate::api::ApiError;
use crate::state::AppState;
use crate::store_runs::AgentSession;

/// How long a new session may show the folder-trust dialog before we stop watching.
const TRUST_WATCH: Duration = Duration::from_secs(60);

#[derive(Debug, Deserialize)]
pub struct RunRequest {
    pub key: String,
    pub title: String,
    #[serde(default = "default_workflow")]
    pub workflow: String,
    /// Main checkout of the parent repo.
    pub parent: PathBuf,
    pub submodules: Vec<String>,
    /// Defaults to `feature/<key>`.
    pub branch: Option<String>,
    /// Defaults to `/feature <key>` for the `feature` workflow.
    pub prompt: Option<String>,
}

fn default_workflow() -> String {
    "feature".into()
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
    let prompt = match (&req.prompt, req.workflow.as_str()) {
        (Some(prompt), _) => prompt.clone(),
        (None, "feature") => format!("/feature {}", req.key),
        (None, other) => {
            return Err(ApiError::bad_request(format!(
                "workflow `{other}` needs a --prompt"
            )));
        }
    };

    let key = TaskKey::new(&req.key);
    app.store
        .create_task(&Task::new(key.clone(), &req.title, &req.workflow))
        .await?;
    app.store
        .apply_event(&key, Event::Start, Utc::now())
        .await?;

    let spec = WorkspaceSpec {
        path: app.config.workspaces_dir().join(&req.key),
        branch: req
            .branch
            .clone()
            .unwrap_or_else(|| format!("feature/{}", req.key)),
        submodules: req.submodules.clone(),
    };
    let opts = GitOptions {
        allow_file_protocol: app.config.allow_file_protocol,
    };
    let parent = req.parent.clone();
    let created =
        tokio::task::spawn_blocking(move || delilah_git::create_workspace(&parent, &spec, &opts))
            .await
            .map_err(|e| ApiError::internal(e.to_string()))?;
    let ws = match created {
        Ok(ws) => ws,
        Err(e) => {
            let reason = format!("workspace: {e}");
            app.store
                .apply_event(
                    &key,
                    Event::Fail {
                        reason: reason.clone(),
                    },
                    Utc::now(),
                )
                .await?;
            return Err(ApiError::unprocessable(reason));
        }
    };
    app.store.save_workspace(&key, &req.parent, &ws).await?;
    app.store
        .apply_event(&key, Event::WorkspaceReady, Utc::now())
        .await?;

    let session = AgentSession {
        agent: "claude".into(),
        session_id: uuid::Uuid::new_v4().to_string(),
    };
    app.store.save_agent_session(&key, &session).await?;
    launch_claude(
        &app,
        &key,
        &ws.path,
        &session.session_id,
        Some(&prompt),
        false,
    )?;

    let task = app
        .store
        .get_task(&key)
        .await?
        .expect("task was just created");
    Ok((StatusCode::CREATED, Json(task)))
}

/// Starts (or resumes) Claude for `key` in a daemon-owned PTY.
pub fn launch_claude(
    app: &AppState,
    key: &TaskKey,
    workspace: &std::path::Path,
    session_id: &str,
    prompt: Option<&str>,
    resume: bool,
) -> Result<(), ApiError> {
    let task_dir = app.config.task_dir(key.as_str());
    let settings = claude::write_settings(
        &task_dir.join("claude-settings.json"),
        &app.config.url(),
        key.as_str(),
    )
    .map_err(|e| ApiError::internal(format!("cannot write agent settings: {e}")))?;
    let cmd = claude::command(&claude::Launch {
        workspace,
        session_id,
        settings: &settings,
        prompt,
        resume,
    });
    let pty = PtySession::spawn(cmd, 120, 40, Some(task_dir.join("terminal.log")))
        .map_err(|e| ApiError::internal(format!("cannot start claude: {e}")))?;
    let live = app.sessions.insert(key.as_str(), Arc::clone(&pty));
    tracing::info!(task = %key, session_id, resume, "claude started");

    tokio::spawn(accept_trust_dialog(
        Arc::clone(&pty),
        Arc::clone(&live.started),
    ));
    let sessions = Arc::clone(&app.sessions);
    let key = key.to_string();
    tokio::spawn(async move {
        pty.wait().await;
        sessions.remove_if_same(&key, &pty);
        tracing::info!(task = %key, "claude exited");
    });
    Ok(())
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
