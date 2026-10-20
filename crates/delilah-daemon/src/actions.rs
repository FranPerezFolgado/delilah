//! Things the user asks of a task: resume it, mark it done, archive it, delete it.

use std::time::Duration;

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
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
    release_workspace(&app, &key, q.force).await?;
    app.store.delete_workspace(&key).await?;
    app.emit(Topic::Tasks);
    app.emit(Topic::Specs);
    Ok(Json(task))
}

/// Stops the task's sessions and lets go of its workspace: a task in the project's
/// checkout goes back to where it was, a worktree goes away (unless work would be
/// lost and not `force`). The branch stays. Nothing to do if it has none (e.g. its
/// checkout was taken by a newer task). Its record stays for the caller to forget:
/// a deleted task takes it in the same statement as the task.
async fn release_workspace(app: &AppState, key: &TaskKey, force: bool) -> Result<(), ApiError> {
    // Its sessions stop even without a workspace: nothing else would track them.
    for live in app.sessions.of_task(key.as_str()) {
        let _ = live.pty.kill();
        live.pty.wait().await;
    }
    let Some((root, ws)) = app.store.workspace(key).await? else {
        return Ok(());
    };
    let opts = GitOptions {
        allow_file_protocol: app.config.allow_file_protocol,
    };
    // A task in the project's checkout goes back to where it was; a worktree goes away.
    let previous = app.store.checkout_previous(key).await?;
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
        None => delilah_git::archive_workspace(&root, &ws, force, &opts),
    })
    .await
    .map_err(|e| ApiError::internal(e.to_string()))?
    .map_err(|e| ApiError::conflict(e.to_string()))?;
    Ok(())
}

#[derive(Debug, Default, serde::Deserialize)]
pub struct DeleteQuery {
    /// Delete it even if work in its worktree is lost (the user confirmed).
    #[serde(default)]
    pub force: bool,
}

/// `DELETE /api/v1/tasks/{key}`: forgets a task for good, in any state but while its
/// workspace is being prepared. Work in a worktree that would be lost refuses it
/// first, with nothing stopped yet; then its agents stop, the workspace is released
/// as on archive (the branch stays) and the task goes with its history and sessions,
/// in one statement: after that it is nowhere.
pub async fn delete(
    State(app): State<AppState>,
    Path(key): Path<String>,
    axum::extract::Query(q): axum::extract::Query<DeleteQuery>,
) -> Result<StatusCode, ApiError> {
    let key = TaskKey::new(key);
    let task = app
        .store
        .get_task(&key)
        .await?
        .ok_or(StoreError::NotFound(key.clone()))?;
    if task.status == TaskStatus::PreparingWorkspace {
        return Err(ApiError::conflict(format!(
            "{key} is preparing its workspace; try again in a few seconds"
        )));
    }
    // A checkout keeps the branch and its folder: only a worktree can lose work.
    if !q.force
        && let Some((_, ws)) = app.store.workspace(&key).await?
        && app.store.checkout_previous(&key).await?.is_none()
    {
        let opts = GitOptions {
            allow_file_protocol: app.config.allow_file_protocol,
        };
        tokio::task::spawn_blocking(move || delilah_git::check_workspace(&ws, &opts))
            .await
            .map_err(|e| ApiError::internal(e.to_string()))?
            .map_err(|e| ApiError::unpushed(format!("{key} would lose work: {e}")))?;
    }
    if task.status == TaskStatus::Queued {
        // No workspace yet; deleted only if still in the backlog, so a start that
        // slipped in since it was read is never left without its task.
        app.store
            .delete_backlog_task(&key)
            .await
            .map_err(|e| match e {
                StoreError::NotInBacklog(_) => {
                    ApiError::conflict(format!("{key} has just been started; try again"))
                }
                e => e.into(),
            })?;
    } else {
        // Already checked (or forced): the release no longer asks.
        release_workspace(&app, &key, true).await?;
        app.store.delete_task(&key).await?;
    }

    app.asks.release_task(key.as_str());
    crate::phases::drop_hint(&app, key.as_str());
    app.specs.lock().unwrap().remove(key.as_str());
    let dir = app.config.task_dir(key.as_str());
    if let Err(e) = std::fs::remove_dir_all(&dir)
        && e.kind() != std::io::ErrorKind::NotFound
    {
        tracing::warn!(task = %key, "cannot remove {}: {e}", dir.display());
    }
    tracing::info!(task = %key, "task deleted");
    for topic in [Topic::Tasks, Topic::Sessions, Topic::Inbox, Topic::Specs] {
        app.emit(topic);
    }
    Ok(StatusCode::NO_CONTENT)
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
    use std::path::{Path as FsPath, PathBuf};

    fn git(dir: &FsPath, args: &[&str]) -> String {
        let out = std::process::Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(["-c", "user.name=t", "-c", "user.email=t@t"])
            .args(args)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    }

    /// A daemon whose home is a temp folder, and a repo there with an origin.
    async fn setup() -> (AppState, PathBuf) {
        let home = std::env::temp_dir().join(format!("delilah-del-{}", uuid::Uuid::new_v4()));
        let origin = home.join("origin.git");
        let repo = home.join("web");
        std::fs::create_dir_all(&home).unwrap();
        git(
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
        git(
            &home,
            &[
                "clone",
                "-q",
                origin.to_str().unwrap(),
                repo.to_str().unwrap(),
            ],
        );
        std::fs::write(repo.join("README.md"), "web\n").unwrap();
        git(&repo, &["add", "."]);
        git(&repo, &["commit", "-qm", "init"]);
        git(&repo, &["push", "-q", "origin", "HEAD:main"]);
        let config = crate::config::Config {
            home: home.clone(),
            addr: crate::config::DEFAULT_ADDR.parse().unwrap(),
            allow_file_protocol: false,
            notify: false,
            ui_origin: None,
        };
        (
            AppState::new(crate::store::Store::in_memory().await.unwrap(), config),
            repo,
        )
    }

    /// A running task with its workspace (a worktree, or the repo's checkout).
    async fn started(
        app: &AppState,
        repo: &FsPath,
        key: &str,
        in_checkout: bool,
    ) -> delilah_git::Workspace {
        let key = TaskKey::new(key);
        app.store
            .create_task(&Task::new(key.clone(), "x", "feature"))
            .await
            .unwrap();
        app.store
            .apply_event(&key, Event::Start, chrono::Utc::now())
            .await
            .unwrap();
        let spec = delilah_git::WorkspaceSpec {
            path: app.config.workspaces_dir().join(key.as_str()),
            branch: format!("feature/{key}"),
            kind: delilah_core::ProjectKind::Repo,
            submodules: vec![],
        };
        let opts = GitOptions::default();
        let ws = if in_checkout {
            let (ws, previous) = delilah_git::use_checkout(repo, &spec, &opts).unwrap();
            app.store.save_workspace(&key, repo, &ws).await.unwrap();
            app.store
                .set_checkout_previous(&key, &previous)
                .await
                .unwrap();
            ws
        } else {
            let ws = delilah_git::create_workspace(repo, &spec, &opts).unwrap();
            app.store.save_workspace(&key, repo, &ws).await.unwrap();
            ws
        };
        app.store
            .apply_event(&key, Event::WorkspaceReady, chrono::Utc::now())
            .await
            .unwrap();
        std::fs::create_dir_all(app.config.task_dir(key.as_str())).unwrap();
        ws
    }

    async fn delete_task(app: &AppState, key: &str, force: bool) -> Result<StatusCode, ApiError> {
        delete(
            State(app.clone()),
            Path(key.to_string()),
            axum::extract::Query(DeleteQuery { force }),
        )
        .await
    }

    #[tokio::test]
    async fn deletes_a_backlog_task_and_frees_its_key() {
        let (app, _) = setup().await;
        let key = TaskKey::new("PROJ-1");
        app.store
            .create_task(&Task::new(key.clone(), "x", "feature"))
            .await
            .unwrap();

        assert_eq!(
            delete_task(&app, "PROJ-1", false).await.ok(),
            Some(StatusCode::NO_CONTENT)
        );
        assert!(app.store.get_task(&key).await.unwrap().is_none());
        let err = delete_task(&app, "PROJ-1", false).await.unwrap_err();
        assert_eq!(err.status, StatusCode::NOT_FOUND);
        app.store
            .create_task(&Task::new(key, "again", "feature"))
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn waits_for_a_workspace_being_prepared() {
        let (app, _) = setup().await;
        let key = TaskKey::new("PROJ-1");
        app.store
            .create_task(&Task::new(key.clone(), "x", "feature"))
            .await
            .unwrap();
        app.store
            .apply_event(&key, Event::Start, chrono::Utc::now())
            .await
            .unwrap();
        let err = delete_task(&app, "PROJ-1", false).await.unwrap_err();
        assert_eq!(err.status, StatusCode::CONFLICT);
        assert!(app.store.get_task(&key).await.unwrap().is_some());
    }

    #[tokio::test]
    async fn unpushed_work_is_lost_only_when_forced() {
        let (app, repo) = setup().await;
        let ws = started(&app, &repo, "PROJ-1", false).await;
        std::fs::write(ws.path.join("retries.rs"), "// wip\n").unwrap();
        git(&ws.path, &["add", "."]);
        git(&ws.path, &["commit", "-qm", "wip"]);

        let err = delete_task(&app, "PROJ-1", false).await.unwrap_err();
        assert_eq!(
            (err.status, err.unpushed),
            (StatusCode::CONFLICT, true),
            "{}",
            err.message
        );
        let key = TaskKey::new("PROJ-1");
        assert!(
            app.store.get_task(&key).await.unwrap().is_some(),
            "nothing was touched"
        );
        assert!(app.store.workspace(&key).await.unwrap().is_some());
        assert!(ws.path.exists());

        assert_eq!(
            delete_task(&app, "PROJ-1", true).await.ok(),
            Some(StatusCode::NO_CONTENT)
        );
        assert!(app.store.get_task(&key).await.unwrap().is_none());
        assert!(!ws.path.exists());
        assert!(!app.config.task_dir("PROJ-1").exists());
        // The branch, with its commit, stays in the repo.
        assert!(git(&repo, &["branch", "--list", "feature/PROJ-1"]).contains("PROJ-1"));
    }

    #[tokio::test]
    async fn a_checkout_task_goes_back_to_the_previous_branch() {
        let (app, repo) = setup().await;
        started(&app, &repo, "PROJ-1", true).await;
        assert_eq!(git(&repo, &["branch", "--show-current"]), "feature/PROJ-1");

        assert_eq!(
            delete_task(&app, "PROJ-1", false).await.ok(),
            Some(StatusCode::NO_CONTENT)
        );
        assert_eq!(git(&repo, &["branch", "--show-current"]), "main");
        assert!(git(&repo, &["branch", "--list", "feature/PROJ-1"]).contains("PROJ-1"));
    }

    #[tokio::test]
    async fn stops_the_agent_of_a_running_task() {
        let (app, repo) = setup().await;
        let ws = started(&app, &repo, "PROJ-1", false).await;
        let mut cmd = portable_pty::CommandBuilder::new("sleep");
        cmd.arg("60");
        let pty = delilah_agents::pty::PtySession::spawn(cmd, 80, 24, None).unwrap();
        app.sessions.insert(
            "s-1",
            "PROJ-1",
            crate::store_sessions::SessionKind::Claude,
            true,
            pty.clone(),
        );

        assert_eq!(
            delete_task(&app, "PROJ-1", false).await.ok(),
            Some(StatusCode::NO_CONTENT)
        );
        assert!(pty.has_exited(), "the agent was stopped");
        assert!(!ws.path.exists());
    }

    #[tokio::test]
    async fn stops_the_sessions_of_a_task_without_workspace() {
        let (app, _) = setup().await;
        let key = TaskKey::new("PROJ-1");
        app.store
            .create_task(&Task::new(key.clone(), "x", "feature"))
            .await
            .unwrap();
        for event in [Event::Start, Event::WorkspaceReady, Event::MarkDone] {
            app.store
                .apply_event(&key, event, chrono::Utc::now())
                .await
                .unwrap();
        }
        let mut cmd = portable_pty::CommandBuilder::new("sleep");
        cmd.arg("60");
        let pty = delilah_agents::pty::PtySession::spawn(cmd, 80, 24, None).unwrap();
        app.sessions.insert(
            "s-1",
            "PROJ-1",
            crate::store_sessions::SessionKind::Shell,
            false,
            pty.clone(),
        );

        delete_task(&app, "PROJ-1", false).await.ok().unwrap();
        assert!(pty.has_exited());
    }

    #[tokio::test]
    async fn a_worktree_removed_by_hand_does_not_stop_the_delete() {
        let (app, repo) = setup().await;
        let ws = started(&app, &repo, "PROJ-1", false).await;
        std::fs::remove_dir_all(&ws.path).unwrap();

        assert_eq!(
            delete_task(&app, "PROJ-1", false)
                .await
                .map_err(|e| e.message)
                .unwrap(),
            StatusCode::NO_CONTENT
        );
        assert!(
            app.store
                .get_task(&TaskKey::new("PROJ-1"))
                .await
                .unwrap()
                .is_none()
        );
    }

    #[tokio::test]
    async fn a_checkout_taken_by_a_newer_task_is_left_alone() {
        let (app, repo) = setup().await;
        started(&app, &repo, "PROJ-1", true).await;
        // A newer task took the checkout: PROJ-1 has no workspace left.
        app.store
            .delete_workspace(&TaskKey::new("PROJ-1"))
            .await
            .unwrap();
        git(&repo, &["checkout", "-qb", "feature/PROJ-2"]);

        delete_task(&app, "PROJ-1", false).await.ok().unwrap();
        assert_eq!(
            git(&repo, &["branch", "--show-current"]),
            "feature/PROJ-2",
            "the newer task's checkout is untouched"
        );
    }

    #[tokio::test]
    async fn a_backlog_task_started_meanwhile_is_not_deleted() {
        let (app, _) = setup().await;
        let key = TaskKey::new("PROJ-1");
        app.store
            .create_task(&Task::new(key.clone(), "x", "feature"))
            .await
            .unwrap();
        // As if the start got in after the delete read it from the backlog.
        app.store
            .apply_event(&key, Event::Start, chrono::Utc::now())
            .await
            .unwrap();
        let err = app.store.delete_backlog_task(&key).await.unwrap_err();
        assert!(matches!(err, StoreError::NotInBacklog(_)), "{err}");
        assert!(app.store.get_task(&key).await.unwrap().is_some());
    }

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
