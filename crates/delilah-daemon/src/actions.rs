//! Things the user asks of a running task: resume it, mark it done, archive it.

use std::time::Duration;

use axum::Json;
use axum::extract::{Path, State};
use delilah_core::{Event, Task, TaskKey, TaskStatus};
use delilah_git::GitOptions;

use crate::api::ApiError;
use crate::run::launch_session;
use crate::sessions::{Live, LiveState};
use crate::state::{AppState, UsageReport};
use crate::store::StoreError;
use crate::store_sessions::SessionKind;

/// What the agent is told when it picks up after a pause.
pub const CONTINUE_PROMPT: &str =
    "Se ha restablecido el límite de uso. Continúa la tarea donde la dejaste.";

pub async fn resume(
    State(app): State<AppState>,
    Path(key): Path<String>,
) -> Result<Json<Task>, ApiError> {
    let key = TaskKey::new(key);
    let task = app.transition(&key, Event::ManualResume).await?;
    continue_agent(&app, &key).await?;
    Ok(Json(task))
}

pub async fn done(
    State(app): State<AppState>,
    Path(key): Path<String>,
) -> Result<Json<Task>, ApiError> {
    let key = TaskKey::new(key);
    let task = app.transition(&key, Event::MarkDone).await?;
    for live in app.sessions.of_task(key.as_str()) {
        let _ = live.pty.kill();
    }
    Ok(Json(task))
}

/// Removes the workspace of a finished task, refusing while its work is not pushed.
pub async fn archive(
    State(app): State<AppState>,
    Path(key): Path<String>,
) -> Result<Json<Task>, ApiError> {
    let key = TaskKey::new(key);
    let task = app
        .store
        .get_task(&key)
        .await?
        .ok_or(StoreError::NotFound(key.clone()))?;
    if !task.status.is_terminal() {
        return Err(ApiError::conflict(format!(
            "{key} is {}; mark it done first (`delilah done {key}`)",
            task.status.name()
        )));
    }
    let (root, ws) = app
        .store
        .workspace(&key)
        .await?
        .ok_or_else(|| ApiError::not_found(format!("{key} has no workspace")))?;
    for live in app.sessions.of_task(key.as_str()) {
        let _ = live.pty.kill();
        live.pty.wait().await;
    }
    let opts = GitOptions {
        allow_file_protocol: app.config.allow_file_protocol,
    };
    tokio::task::spawn_blocking(move || delilah_git::archive_workspace(&root, &ws, &opts))
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?
        .map_err(|e| ApiError::conflict(e.to_string()))?;
    app.store.delete_workspace(&key).await?;
    Ok(Json(task))
}

pub async fn usage(State(app): State<AppState>) -> Json<Option<UsageReport>> {
    Json(app.usage.lock().unwrap().clone())
}

/// Gets a resumed task's agents going again: the primary one, and any extra Claude
/// session that stopped on the limit too. A live agent is typed to; a primary one
/// that is gone (e.g. after a restart) is started again with `--resume`.
pub async fn continue_agent(app: &AppState, key: &TaskKey) -> Result<(), ApiError> {
    let live = app.sessions.of_task(key.as_str());
    for session in live.iter().filter(|l| l.kind == SessionKind::Claude) {
        if (session.primary || session.state() == LiveState::Paused) && !session.pty.has_exited() {
            type_prompt(session, CONTINUE_PROMPT).await?;
        }
    }
    if live.iter().any(|l| l.primary && !l.pty.has_exited()) {
        return Ok(());
    }
    let primary = app
        .store
        .primary_session(key.as_str())
        .await?
        .ok_or_else(|| ApiError::not_found(format!("{key} has no agent session to resume")))?;
    let (_, ws) = app
        .store
        .workspace(key)
        .await?
        .ok_or_else(|| ApiError::not_found(format!("{key} has no workspace")))?;
    launch_session(app, &primary, &ws.path, Some(CONTINUE_PROMPT), true)
}

/// Writes a message into an agent's terminal as if the user typed it.
pub async fn type_prompt(session: &Live, text: &str) -> Result<(), ApiError> {
    session
        .pty
        .write(text.as_bytes())
        .map_err(|e| ApiError::internal(e.to_string()))?;
    // Sent apart so the TUI takes it as Enter, not as part of a paste.
    tokio::time::sleep(Duration::from_millis(300)).await;
    session
        .pty
        .write(b"\r")
        .map_err(|e| ApiError::internal(e.to_string()))?;
    session.set_state(LiveState::Working);
    Ok(())
}

/// Resumes, from the scheduler, a paused task whose reset time has passed.
pub async fn resume_if_due(app: &AppState, task: &Task) {
    let TaskStatus::PausedByLimit { resume_at } = task.status else {
        return;
    };
    let now = chrono::Utc::now();
    if now < resume_at {
        return;
    }
    match app.transition(&task.key, Event::ResetDue { now }).await {
        Ok(_) => {
            if let Err(e) = continue_agent(app, &task.key).await {
                tracing::warn!(task = %task.key, "could not continue the agent: {}", e.message());
            }
        }
        Err(e) => tracing::warn!(task = %task.key, "could not resume: {e}"),
    }
}
