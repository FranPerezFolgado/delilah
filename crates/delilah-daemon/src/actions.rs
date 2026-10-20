//! Things the user asks of a running task: resume it, mark it done, archive it.

use std::time::Duration;

use axum::Json;
use axum::extract::{Path, State};
use delilah_core::{Event, Task, TaskKey, TaskStatus};
use delilah_git::GitOptions;

use crate::api::ApiError;
use crate::events::Topic;
use crate::run::launch_session;
use crate::sessions::{Live, LiveState};
use crate::state::{AppState, UsageReport};
use crate::store::StoreError;
use crate::store_sessions::SessionKind;

/// What the agent is told when it picks up after a pause.
pub fn continue_prompt(lang: crate::lang::Lang) -> &'static str {
    lang.pick(
        "Se ha restablecido el límite de uso. Continúa la tarea donde la dejaste.",
        "The usage limit has reset. Carry on with the task where you left it.",
    )
}

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

#[derive(Debug, Default, serde::Deserialize)]
pub struct ArchiveQuery {
    /// Remove a worktree even with work that is not pushed (the user confirmed).
    #[serde(default)]
    pub force: bool,
}

/// Removes the workspace of a finished task. A worktree is kept while its work is not
/// pushed, unless `force`; the project's checkout is always let go of.
pub async fn archive(
    State(app): State<AppState>,
    Path(key): Path<String>,
    axum::extract::Query(q): axum::extract::Query<ArchiveQuery>,
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
    // Already released (e.g. its checkout was taken by a newer task): nothing to do.
    let Some((root, ws)) = app.store.workspace(&key).await? else {
        return Ok(Json(task));
    };
    for live in app.sessions.of_task(key.as_str()) {
        let _ = live.pty.kill();
        live.pty.wait().await;
    }
    let opts = GitOptions {
        allow_file_protocol: app.config.allow_file_protocol,
    };
    // A task in the project's checkout goes back to where it was; a worktree goes away.
    let previous = app.store.checkout_previous(&key).await?;
    tokio::task::spawn_blocking(move || match previous {
        Some(previous) => {
            for stuck in delilah_git::leave_checkout(&root, &previous, &opts) {
                tracing::warn!(
                    "archiving {}: left on the task's branch: {stuck}",
                    ws.branch
                );
            }
            Ok(())
        }
        None => delilah_git::archive_workspace(&root, &ws, q.force, &opts),
    })
    .await
    .map_err(|e| ApiError::internal(e.to_string()))?
    .map_err(|e| ApiError::conflict(e.to_string()))?;
    app.store.delete_workspace(&key).await?;
    app.emit(Topic::Tasks);
    app.emit(Topic::Specs);
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
            type_prompt(session, continue_prompt(app.lang())).await?;
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
    launch_session(
        app,
        &primary,
        &ws.path,
        Some(continue_prompt(app.lang())),
        true,
    )
}

/// The bytes that type `text` into the TUI. Several lines go as a bracketed paste,
/// or each newline would send the message early.
fn prompt_bytes(text: &str) -> Vec<u8> {
    let text = text.replace("\r\n", "\n").replace('\r', "\n");
    if !text.contains('\n') {
        return text.into_bytes();
    }
    // Control characters inside the paste (an ESC that starts a paste marker above
    // all) could end it early: only newlines and tabs stay.
    let text: String = text
        .chars()
        .filter(|&c| c == '\n' || c == '\t' || !c.is_control())
        .collect();
    format!("\x1b[200~{text}\x1b[201~").into_bytes()
}

/// Writes a message into an agent's terminal as if the user typed it.
pub async fn type_prompt(session: &Live, text: &str) -> Result<(), ApiError> {
    session
        .pty
        .write(&prompt_bytes(text))
        .map_err(|e| ApiError::internal(e.to_string()))?;
    // Sent apart, after any paste is closed, so the TUI takes it as Enter rather
    // than as one more character of the text.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_line_is_typed_as_is() {
        assert_eq!(prompt_bytes("vale, sigue"), b"vale, sigue");
    }

    #[test]
    fn several_lines_go_as_one_paste() {
        assert_eq!(
            prompt_bytes("uno\r\ndos\nthree"),
            b"\x1b[200~uno\ndos\nthree\x1b[201~"
        );
    }

    #[test]
    fn a_lone_carriage_return_is_a_new_line_too() {
        assert_eq!(prompt_bytes("a\rb"), b"\x1b[200~a\nb\x1b[201~");
    }

    #[test]
    fn a_trailing_new_line_stays_inside_the_paste() {
        assert_eq!(prompt_bytes("a\n"), b"\x1b[200~a\n\x1b[201~");
    }

    #[test]
    fn a_paste_marker_in_the_text_cannot_end_the_paste() {
        assert_eq!(
            prompt_bytes("a\x1b[201~\rb"),
            b"\x1b[200~a[201~\nb\x1b[201~"
        );
        // Removing the marker must not leave another one behind.
        assert_eq!(
            prompt_bytes("a\x1b[20\x1b[201~1~\nb"),
            b"\x1b[200~a[20[201~1~\nb\x1b[201~"
        );
        assert_eq!(prompt_bytes("a\tb\x07\nc"), b"\x1b[200~a\tb\nc\x1b[201~");
    }
}
