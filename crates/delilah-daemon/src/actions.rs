//! Things the user asks of a task: resume it, mark it done, archive it, delete it.

use std::collections::HashMap;

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use delilah_core::{Event, Task, TaskKey, TaskStatus};
use delilah_git::GitOptions;

use crate::api::ApiError;
use crate::archive::{archive_workspace, finish_partial, release_workspace};
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
        let _ = live.runtime.kill();
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
    if !task.status.can_archive() {
        return Err(ApiError::conflict(format!(
            "{key} is {}; mark it done first (`delilah done {key}`)",
            task.status.name()
        )));
    }
    app.end_task_runtime(&key);
    if finish_partial(&app, &key).await? {
        app.emit(Topic::Inbox);
    }
    // A merge may already have released the workspace: then this only stops what runs.
    // (No branch is deleted here: none was verified merged.)
    archive_workspace(&app, &key, q.force, &HashMap::new()).await?;
    app.emit(Topic::Inbox);
    // Nothing will launch from a saved launch any more.
    crate::run::forget_launch(&app, &key);
    app.emit(Topic::Tasks);
    app.emit(Topic::Specs);
    Ok(Json(task))
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
    app.end_task_runtime(&key);
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
    crate::permissions::release_task(&app, key.as_str()).await;
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

/// `POST /tasks/{key}/reopen`: a done task waits for the user again.
pub async fn reopen(
    State(app): State<AppState>,
    Path(key): Path<String>,
) -> Result<Json<Task>, ApiError> {
    let key = TaskKey::new(key);
    app.store
        .get_task(&key)
        .await?
        .ok_or(StoreError::NotFound(key.clone()))?;
    if app.store.workspace(&key).await?.is_none() {
        return Err(ApiError::conflict(format!(
            "{key} has no workspace any more; it cannot be reopened"
        )));
    }
    let task = app.transition(&key, Event::Reopen).await?;
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
        if (session.primary || session.state() == LiveState::Paused)
            && !session.runtime.has_exited()
        {
            type_prompt(session, continue_prompt(app.lang())).await?;
        }
    }
    if live.iter().any(|l| l.primary && !l.runtime.has_exited()) {
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
    .await
}

pub use delilah_agents::runtime::sanitize_prompt;

/// Writes a message into an agent's terminal as if the user typed it.
pub async fn type_prompt(session: &Live, text: &str) -> Result<(), ApiError> {
    // Working before the write: a turn that fails at once (its result handled while the
    // write returns) must not be overwritten by a late `Working`.
    let before = session.state();
    session.set_state(LiveState::Working);
    if let Err(e) = session.runtime.send_prompt(text).await {
        session.set_state(before);
        return Err(ApiError::internal(e.to_string()));
    }
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
    use crate::archive::{ArchivePartial, LeftoverBranch};
    use delilah_agents::runtime::prompt_bytes;

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
            claude_bin: "claude".into(),
            agent_program: None,
            ..crate::config::Config::test_defaults()
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
        started_on(app, repo, key, in_checkout, Some(format!("feature/{key}"))).await
    }

    /// Like [`started`], with the branch given (`None`: the task has none yet).
    async fn started_on(
        app: &AppState,
        repo: &FsPath,
        key: &str,
        in_checkout: bool,
        branch: Option<String>,
    ) -> delilah_git::Workspace {
        let key = TaskKey::new(key);
        app.store
            .create_task(&Task::new(key.clone(), "x", "project"))
            .await
            .unwrap();
        app.store
            .apply_event(&key, Event::Start, chrono::Utc::now())
            .await
            .unwrap();
        let spec = delilah_git::WorkspaceSpec {
            path: app.config.workspaces_dir().join(key.as_str()),
            branch,
            kind: delilah_core::ProjectKind::Repo,
            submodules: vec![],
        };
        let opts = GitOptions::default();
        let ws = if in_checkout {
            let deadline = delilah_git::Deadline::after(std::time::Duration::from_secs(30));
            let (ws, previous, _) =
                delilah_git::use_checkout(repo, &spec, &opts, deadline).unwrap();
            app.store.save_workspace(&key, repo, &ws).await.unwrap();
            app.store
                .set_checkout_previous(&key, &previous)
                .await
                .unwrap();
            ws
        } else {
            let ws = delilah_git::create_workspace(
                repo,
                &spec,
                &opts,
                delilah_git::Deadline::after(std::time::Duration::from_secs(30)),
            )
            .unwrap()
            .0;
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

    /// A task stopped in `setup-failed`, with a worktree and a saved launch.
    async fn setup_failed(app: &AppState, repo: &FsPath, key: &str) -> delilah_git::Workspace {
        let key = TaskKey::new(key);
        app.store
            .create_task(&Task::new(key.clone(), "x", "project"))
            .await
            .unwrap();
        app.store
            .apply_event(&key, Event::Start, chrono::Utc::now())
            .await
            .unwrap();
        let spec = delilah_git::WorkspaceSpec {
            path: app.config.workspaces_dir().join(key.as_str()),
            branch: Some(format!("feature/{key}")),
            kind: delilah_core::ProjectKind::Repo,
            submodules: vec![],
        };
        let (ws, _) = delilah_git::create_workspace(
            repo,
            &spec,
            &GitOptions::default(),
            delilah_git::Deadline::after(std::time::Duration::from_secs(30)),
        )
        .unwrap();
        app.store.save_workspace(&key, repo, &ws).await.unwrap();
        app.store
            .apply_event(
                &key,
                Event::SetupFailed {
                    reason: "boom".into(),
                },
                chrono::Utc::now(),
            )
            .await
            .unwrap();
        let dir = app.config.task_dir(key.as_str());
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(crate::run::LAUNCH_FILE), b"{}").unwrap();
        ws
    }

    #[tokio::test]
    async fn archives_a_setup_failed_task_and_drops_its_saved_launch() {
        let (app, repo) = setup().await;
        let ws = setup_failed(&app, &repo, "PROJ-1").await;
        let res = archive(
            State(app.clone()),
            Path("PROJ-1".into()),
            axum::extract::Query(ArchiveQuery { force: false }),
        )
        .await;
        assert!(res.is_ok());
        assert!(!ws.path.exists());
        let key = TaskKey::new("PROJ-1");
        assert!(app.store.workspace(&key).await.unwrap().is_none());
        assert!(
            !app.config
                .task_dir("PROJ-1")
                .join(crate::run::LAUNCH_FILE)
                .exists()
        );
    }

    #[tokio::test]
    async fn deletes_a_setup_failed_task_and_releases_its_worktree() {
        let (app, repo) = setup().await;
        let ws = setup_failed(&app, &repo, "PROJ-1").await;
        assert_eq!(
            delete_task(&app, "PROJ-1", false).await.ok(),
            Some(StatusCode::NO_CONTENT)
        );
        assert!(!ws.path.exists());
        assert!(!app.config.task_dir("PROJ-1").exists());
        assert!(
            app.store
                .get_task(&TaskKey::new("PROJ-1"))
                .await
                .unwrap()
                .is_none()
        );
    }

    #[tokio::test]
    async fn deletes_a_backlog_task_and_frees_its_key() {
        let (app, _) = setup().await;
        let key = TaskKey::new("PROJ-1");
        app.store
            .create_task(&Task::new(key.clone(), "x", "project"))
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
            .create_task(&Task::new(key, "again", "project"))
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn waits_for_a_workspace_being_prepared() {
        let (app, _) = setup().await;
        let key = TaskKey::new("PROJ-1");
        app.store
            .create_task(&Task::new(key.clone(), "x", "project"))
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
    async fn a_branchless_worktree_goes_away_clean_and_is_refused_dirty_unless_forced() {
        let (app, repo) = setup().await;
        let ws = started_on(&app, &repo, "PROJ-1", false, None).await;
        assert!(!ws.has_branch());
        std::fs::write(ws.path.join("notes.txt"), "wip\n").unwrap();
        let err = delete_task(&app, "PROJ-1", false).await.unwrap_err();
        assert_eq!(err.status, StatusCode::CONFLICT, "{}", err.message);
        assert!(ws.path.exists());
        assert!(
            app.store
                .workspace(&TaskKey::new("PROJ-1"))
                .await
                .unwrap()
                .is_some()
        );

        assert_eq!(
            delete_task(&app, "PROJ-1", true).await.ok(),
            Some(StatusCode::NO_CONTENT)
        );
        assert!(!ws.path.exists());

        let ws = started_on(&app, &repo, "PROJ-2", false, None).await;
        assert_eq!(
            delete_task(&app, "PROJ-2", false).await.ok(),
            Some(StatusCode::NO_CONTENT)
        );
        assert!(!ws.path.exists());
    }

    #[tokio::test]
    async fn a_branchless_checkout_task_goes_back_to_the_previous_branch() {
        let (app, repo) = setup().await;
        started_on(&app, &repo, "PROJ-1", true, None).await;
        assert_eq!(git(&repo, &["branch", "--show-current"]), "main");
        assert_eq!(
            delete_task(&app, "PROJ-1", false).await.ok(),
            Some(StatusCode::NO_CONTENT)
        );
        assert_eq!(git(&repo, &["branch", "--show-current"]), "main");
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
            .create_task(&Task::new(key.clone(), "x", "project"))
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
            .create_task(&Task::new(key.clone(), "x", "project"))
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

    #[tokio::test]
    async fn archiving_resolves_the_open_pr_notices_of_the_task() {
        use crate::store_prs::NewNotice;
        let (app, repo) = setup().await;
        started(&app, &repo, "PROJ-1", false).await;
        let key = TaskKey::new("PROJ-1");
        for (repo, kind) in [(".", "ci-failed"), ("api", "comments")] {
            app.store
                .insert_notice(
                    &key,
                    &NewNotice {
                        repo: repo.into(),
                        kind: kind.into(),
                        cause: "c".into(),
                        text: "t".into(),
                        detail: None,
                    },
                )
                .await
                .unwrap();
        }
        let mut rx = app.events.subscribe();
        let _ = done(State(app.clone()), Path("PROJ-1".into()))
            .await
            .ok()
            .unwrap();
        let _ = archive(
            State(app.clone()),
            Path("PROJ-1".into()),
            axum::extract::Query(ArchiveQuery { force: true }),
        )
        .await
        .ok()
        .unwrap();
        assert!(app.store.open_notices().await.unwrap().is_empty());
        let mut topics = vec![];
        while let Ok(t) = rx.try_recv() {
            topics.push(t);
        }
        assert!(topics.contains(&Topic::Inbox));
    }

    /// A done task without workspace and an open archive-partial notice for `branch`.
    async fn partially_archived(app: &AppState, repo: &FsPath, branch: &str) -> TaskKey {
        use crate::store_prs::NewNotice;
        let key = TaskKey::new("PROJ-1");
        app.store
            .create_project(&delilah_core::Project {
                name: "web".into(),
                path: repo.to_path_buf(),
                kind: delilah_core::ProjectKind::Repo,
                workflow: "project".into(),
                settings: Default::default(),
                preview_command: None,
            })
            .await
            .unwrap();
        let mut task = Task::new(key.clone(), "x", "project");
        task.project = Some("web".into());
        app.store.create_task(&task).await.unwrap();
        for event in [Event::Start, Event::WorkspaceReady, Event::MarkDone] {
            app.store
                .apply_event(&key, event, chrono::Utc::now())
                .await
                .unwrap();
        }
        let detail = ArchivePartial {
            branches: vec![LeftoverBranch {
                repo: ".".into(),
                branch: branch.into(),
                sha: git(repo, &["rev-parse", &format!("refs/heads/{branch}")]),
            }],
        };
        app.store
            .insert_notice(
                &key,
                &NewNotice {
                    repo: ".".into(),
                    kind: "archive-partial".into(),
                    cause: "merge".into(),
                    text: "t".into(),
                    detail: Some(serde_json::to_value(detail).unwrap()),
                },
            )
            .await
            .unwrap();
        key
    }

    async fn archive_task(app: &AppState, key: &TaskKey) -> Result<Json<Task>, ApiError> {
        archive(
            State(app.clone()),
            Path(key.to_string()),
            axum::extract::Query(ArchiveQuery::default()),
        )
        .await
    }

    /// A done task with a parent-kind worktree (no submodules) whose own repo has the
    /// branch `feature/PROJ-1` with one commit that is not pushed.
    async fn parent_with_a_branch(
        app: &AppState,
        repo: &FsPath,
    ) -> (TaskKey, delilah_git::Workspace) {
        let key = TaskKey::new("PROJ-1");
        app.store
            .create_task(&Task::new(key.clone(), "x", "project"))
            .await
            .unwrap();
        app.store
            .apply_event(&key, Event::Start, chrono::Utc::now())
            .await
            .unwrap();
        let spec = delilah_git::WorkspaceSpec {
            path: app.config.workspaces_dir().join("PROJ-1"),
            branch: None,
            kind: delilah_core::ProjectKind::Parent,
            submodules: vec![],
        };
        let opts = GitOptions::default();
        let (mut ws, _) = delilah_git::create_workspace(
            repo,
            &spec,
            &opts,
            delilah_git::Deadline::after(delilah_git::NETWORK_LIMIT),
        )
        .unwrap();
        delilah_git::apply_branch(&ws.path, "feature/PROJ-1", &opts).unwrap();
        ws.branch = "feature/PROJ-1".into();
        app.store.save_workspace(&key, repo, &ws).await.unwrap();
        app.store
            .apply_event(&key, Event::WorkspaceReady, chrono::Utc::now())
            .await
            .unwrap();
        git(&ws.path, &["commit", "-q", "--allow-empty", "-m", "work"]);
        (key, ws)
    }

    #[tokio::test]
    async fn a_parent_archive_checks_the_branch_in_the_parents_own_repo() {
        let (app, repo) = setup().await;
        let (key, ws) = parent_with_a_branch(&app, &repo).await;

        let err = archive_workspace(&app, &key, false, &HashMap::new())
            .await
            .unwrap_err();
        assert_eq!(err.status, StatusCode::CONFLICT, "{}", err.message);
        assert!(ws.path.exists());

        git(&ws.path, &["push", "-q", "origin", "feature/PROJ-1"]);
        archive_workspace(&app, &key, false, &HashMap::new())
            .await
            .map_err(|e| e.message)
            .unwrap();
        assert!(!ws.path.exists());
        // The user's archive keeps the branch.
        assert!(!git(&repo, &["branch", "--list", "feature/PROJ-1"]).is_empty());
    }

    #[tokio::test]
    async fn a_merge_archive_deletes_the_parents_own_branch_too() {
        let (app, repo) = setup().await;
        let (key, ws) = parent_with_a_branch(&app, &repo).await;
        git(&ws.path, &["push", "-q", "origin", "feature/PROJ-1"]);

        // A submodule's PR merged (no such repo is set up here: nothing to delete there).
        let verified = HashMap::from([("svc".to_string(), "0".repeat(40))]);
        let outcome = archive_workspace(&app, &key, false, &verified)
            .await
            .map_err(|e| e.message)
            .unwrap();
        assert!(outcome.left.is_empty(), "{:?}", outcome.problems);
        assert!(git(&repo, &["branch", "--list", "feature/PROJ-1"]).is_empty());
    }

    fn start_preview(app: &AppState, key: &str) {
        app.previews
            .start(
                key,
                &std::env::temp_dir(),
                "echo http://127.0.0.1:9/x; sleep 30",
            )
            .unwrap();
    }

    #[tokio::test]
    async fn archive_stops_the_preview() {
        let (app, repo) = setup().await;
        git(&repo, &["branch", "feature/PROJ-1"]);
        let key = partially_archived(&app, &repo, "feature/PROJ-1").await;
        start_preview(&app, key.as_str());
        let _ = archive_task(&app, &key)
            .await
            .map_err(|e| e.message)
            .unwrap();
        assert!(app.previews.view(key.as_str()).is_none());
    }

    #[tokio::test]
    async fn delete_stops_the_preview() {
        let (app, repo) = setup().await;
        started(&app, &repo, "PROJ-2", false).await;
        start_preview(&app, "PROJ-2");
        delete_task(&app, "PROJ-2", true).await.ok().unwrap();
        assert!(app.previews.view("PROJ-2").is_none());
    }

    #[tokio::test]
    async fn deleting_a_backlog_task_stops_its_preview() {
        let (app, _) = setup().await;
        app.store
            .create_task(&Task::new(TaskKey::new("PROJ-3"), "x", "project"))
            .await
            .unwrap();
        start_preview(&app, "PROJ-3");
        delete_task(&app, "PROJ-3", false).await.ok().unwrap();
        assert!(app.previews.view("PROJ-3").is_none());
    }

    #[tokio::test]
    async fn archive_finishes_an_archive_partial() {
        let (app, repo) = setup().await;
        git(&repo, &["branch", "feature/PROJ-1"]);
        let key = partially_archived(&app, &repo, "feature/PROJ-1").await;

        let _ = archive_task(&app, &key)
            .await
            .map_err(|e| e.message)
            .unwrap();
        assert!(git(&repo, &["branch", "--list", "feature/PROJ-1"]).is_empty());
        assert!(app.store.open_notices().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn archive_keeps_the_notice_when_a_branch_cannot_go() {
        let (app, repo) = setup().await;
        git(&repo, &["checkout", "-qb", "feature/PROJ-1"]);
        let key = partially_archived(&app, &repo, "feature/PROJ-1").await;

        let err = archive_task(&app, &key).await.unwrap_err();
        assert_eq!(err.status, StatusCode::CONFLICT);
        assert!(git(&repo, &["branch", "--list", "feature/PROJ-1"]).contains("PROJ-1"));
        assert_eq!(app.store.open_notices().await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn archive_keeps_the_notice_when_it_cannot_be_read() {
        let (app, repo) = setup().await;
        git(&repo, &["branch", "feature/PROJ-1"]);
        let key = partially_archived(&app, &repo, "feature/PROJ-1").await;
        // The old shape: an absolute folder instead of a repo label.
        let notice = app.store.open_notices().await.unwrap().remove(0);
        app.store.resolve_notice(notice.id).await.unwrap();
        app.store
            .insert_notice(
                &key,
                &crate::store_prs::NewNotice {
                    repo: ".".into(),
                    kind: "archive-partial".into(),
                    cause: "other".into(),
                    text: "t".into(),
                    detail: Some(serde_json::json!({"branches": [
                        {"dir": repo.to_str().unwrap(), "branch": "feature/PROJ-1"}
                    ]})),
                },
            )
            .await
            .unwrap();

        let err = archive_task(&app, &key).await.unwrap_err();
        assert_eq!(err.status, StatusCode::CONFLICT);
        assert!(git(&repo, &["branch", "--list", "feature/PROJ-1"]).contains("PROJ-1"));
        assert_eq!(app.store.open_notices().await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn archive_never_deletes_a_branch_that_moved_or_a_label_outside_the_root() {
        let (app, repo) = setup().await;
        git(&repo, &["branch", "feature/PROJ-1"]);
        let key = partially_archived(&app, &repo, "feature/PROJ-1").await;
        // The branch gets a commit after the merge was verified.
        git(&repo, &["checkout", "-q", "feature/PROJ-1"]);
        git(&repo, &["commit", "--allow-empty", "-qm", "later"]);
        git(&repo, &["checkout", "-q", "main"]);

        let err = archive_task(&app, &key).await.unwrap_err();
        assert_eq!(err.status, StatusCode::CONFLICT);
        assert!(git(&repo, &["branch", "--list", "feature/PROJ-1"]).contains("PROJ-1"));

        let notice = app.store.open_notices().await.unwrap().remove(0);
        app.store.resolve_notice(notice.id).await.unwrap();
        app.store
            .insert_notice(
                &key,
                &crate::store_prs::NewNotice {
                    repo: ".".into(),
                    kind: "archive-partial".into(),
                    cause: "escape".into(),
                    text: "t".into(),
                    detail: serde_json::to_value(ArchivePartial {
                        branches: vec![LeftoverBranch {
                            repo: "..".into(),
                            branch: "feature/PROJ-1".into(),
                            sha: "0".repeat(40),
                        }],
                    })
                    .ok(),
                },
            )
            .await
            .unwrap();
        let err = archive_task(&app, &key).await.unwrap_err();
        assert_eq!(err.status, StatusCode::CONFLICT);
        assert_eq!(app.store.open_notices().await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn reopening_a_done_task_makes_it_wait_and_records_it() {
        let (app, repo) = setup().await;
        let ws = started(&app, &repo, "PROJ-1", false).await;
        let _ = ws;
        app.transition(&TaskKey::new("PROJ-1"), Event::MarkDone)
            .await
            .unwrap();
        let Json(task) = reopen(State(app.clone()), Path("PROJ-1".into()))
            .await
            .map_err(|e| e.message)
            .unwrap();
        assert_eq!(task.status, TaskStatus::WaitingForUser);
        let events = app.store.events(&TaskKey::new("PROJ-1")).await.unwrap();
        assert_eq!(events.last().unwrap().event, Event::Reopen);
    }

    #[tokio::test]
    async fn reopen_is_409_unless_done() {
        let (app, repo) = setup().await;
        started(&app, &repo, "PROJ-1", false).await;
        let err = reopen(State(app.clone()), Path("PROJ-1".into()))
            .await
            .map(|_| ())
            .unwrap_err();
        assert_eq!(err.status, StatusCode::CONFLICT);
        let err = reopen(State(app.clone()), Path("NOPE-1".into()))
            .await
            .map(|_| ())
            .unwrap_err();
        assert_eq!(err.status, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn reopen_is_409_after_archiving() {
        let (app, repo) = setup().await;
        git(&repo, &["branch", "feature/PROJ-1"]);
        let key = partially_archived(&app, &repo, "feature/PROJ-1").await;
        let _ = archive_task(&app, &key).await.map_err(|e| e.message);
        let err = reopen(State(app.clone()), Path(key.to_string()))
            .await
            .map(|_| ())
            .unwrap_err();
        assert_eq!(err.status, StatusCode::CONFLICT);
    }
}
