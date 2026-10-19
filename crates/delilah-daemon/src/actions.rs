//! Things the user asks of a running task: resume it, mark it done, archive it.

use std::time::Duration;

use axum::Json;
use axum::extract::{Path, State};
use delilah_core::{Event, Task, TaskKey, TaskStatus};
use delilah_git::GitOptions;

use crate::api::ApiError;
use crate::run::launch_claude;
use crate::state::{AppState, UsageReport};
use crate::store::StoreError;

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
    if let Some(live) = app.sessions.get(key.as_str()) {
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
    if let Some(live) = app.sessions.get(key.as_str()) {
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

/// Gets a resumed task's agent going again: types into its terminal if it is still
/// alive, otherwise starts it again with `--resume` on the same session.
pub async fn continue_agent(app: &AppState, key: &TaskKey) -> Result<(), ApiError> {
    if let Some(live) = app.sessions.get(key.as_str())
        && !live.pty.has_exited()
    {
        live.pty
            .write(CONTINUE_PROMPT.as_bytes())
            .map_err(|e| ApiError::internal(e.to_string()))?;
        // Sent apart so the TUI takes it as Enter, not as part of a paste.
        tokio::time::sleep(Duration::from_millis(300)).await;
        live.pty
            .write(b"\r")
            .map_err(|e| ApiError::internal(e.to_string()))?;
        return Ok(());
    }
    let session = app
        .store
        .agent_session(key)
        .await?
        .ok_or_else(|| ApiError::not_found(format!("{key} has no agent session to resume")))?;
    let (_, ws) = app
        .store
        .workspace(key)
        .await?
        .ok_or_else(|| ApiError::not_found(format!("{key} has no workspace")))?;
    launch_claude(
        app,
        key,
        &ws.path,
        &session.session_id,
        Some(CONTINUE_PROMPT),
        true,
    )
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
