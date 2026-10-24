//! `POST /api/v1/tasks/{key}/runtime`: switches a task between chat mode and the
//! terminal. Its live Claude sessions are stopped and started again with `--resume` in
//! the other runtime, so the conversation goes on.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use delilah_core::TaskKey;
use serde::Deserialize;

use crate::api::ApiError;
use crate::events::Topic;
use crate::run::launch_session;
use crate::sessions::{Live, LiveState};
use crate::state::AppState;
use crate::store::StoreError;
use crate::store_sessions::{SessionKind, SessionRecord};

const BUSY: &str = "wait for the turn to end or press Detener";

#[derive(Debug, Deserialize)]
pub struct SetRuntime {
    pub mode: String,
}

pub async fn set_runtime(
    State(app): State<AppState>,
    Path(key): Path<String>,
    Json(body): Json<SetRuntime>,
) -> Result<StatusCode, ApiError> {
    let chat = match body.mode.as_str() {
        "chat" => true,
        "terminal" => false,
        other => {
            return Err(ApiError::bad_request(format!(
                "unknown mode {other:?}: use \"chat\" or \"terminal\""
            )));
        }
    };
    let task_key = TaskKey::new(&key);
    let task = app
        .store
        .get_task(&task_key)
        .await?
        .ok_or(StoreError::NotFound(task_key.clone()))?;

    // The task's own Claude sessions that are running; shells and ideas stay as they are.
    let mut live: Vec<(Live, SessionRecord)> = Vec::new();
    for l in app.sessions.of_task(&key) {
        if l.kind != SessionKind::Claude || l.is_idea() || l.runtime.has_exited() {
            continue;
        }
        if let Some(record) = app.store.session(&l.id).await?.filter(|r| !r.closed) {
            live.push((l, record));
        }
    }
    if task.chat_mode == chat
        && live
            .iter()
            .all(|(l, _)| l.runtime.as_stream().is_some() == chat)
    {
        return Ok(StatusCode::NO_CONTENT);
    }
    let busy = live.iter().any(|(l, _)| {
        l.state() == LiveState::Working
            || app.asks.has_session(&l.id)
            || app.permissions.has_session(&l.id)
    });
    if busy {
        return Err(ApiError::conflict(BUSY));
    }

    let previous = task.chat_mode;
    app.store.set_chat_mode(&task_key, chat).await?;
    app.emit(Topic::Tasks);
    if live.is_empty() {
        return Ok(StatusCode::NO_CONTENT);
    }
    let outcome = relaunch_all(&app, &key, &live).await;
    if let Err(e) = outcome {
        // Back to how it was: the flag first, then every session as it ran.
        if let Err(s) = app.store.set_chat_mode(&task_key, previous).await {
            tracing::warn!(task = %key, "cannot put chat mode back: {s}");
        }
        if let Err(back) = relaunch_all(&app, &key, &live).await {
            tracing::warn!(task = %key, "cannot restore the sessions: {}", back.message());
        }
        app.emit(Topic::Tasks);
        app.emit(Topic::Sessions);
        return Err(e);
    }
    app.emit(Topic::Sessions);
    Ok(StatusCode::NO_CONTENT)
}

/// Stops each session and starts it again with `--resume`, in the runtime the task's
/// flag says now. Stops at the first that fails to come up.
async fn relaunch_all(
    app: &AppState,
    key: &str,
    sessions: &[(Live, SessionRecord)],
) -> Result<(), ApiError> {
    let ws = crate::owners::workspace(app, key)
        .await
        .map_err(|_| ApiError::conflict(format!("{key} has no workspace (archived?)")))?;
    for (_, record) in sessions {
        // Whatever is running now (the original, or what a failed attempt left).
        if let Some(current) = app.sessions.get(&record.id) {
            crate::permissions::release_session(app, &record.id).await;
            let _ = current.runtime.kill();
            current.runtime.wait().await;
            app.sessions.remove_if_same(&record.id, &current.runtime);
        }
        launch_session(app, record, &ws.path, None, true).await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::PermissionsExt;
    use std::sync::Arc;

    use axum::body::Body;
    use axum::http::Request;
    use delilah_core::{Event, Task};
    use delilah_git::Workspace;
    use serde_json::{Value, json};
    use tower::ServiceExt;

    use super::*;
    use crate::config::Config;
    use crate::store::Store;

    /// A running task `PROJ-1` (chat mode as asked) with a workspace, a primary Claude
    /// session `main`, and a `claude` that is `script`; `main` is live on a fake stream
    /// (chat) or PTY (terminal).
    async fn task_in(chat: bool, script: &str) -> AppState {
        let home = std::env::temp_dir().join(format!("delilah-runtime-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(home.join("ws")).unwrap();
        let bin = home.join("fake-claude.sh");
        std::fs::write(&bin, format!("#!/bin/sh\n{script}\n")).unwrap();
        std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
        let config = Config {
            home: home.clone(),
            addr: "127.0.0.1:7420".parse().unwrap(),
            allow_file_protocol: false,
            notify: false,
            ui_origin: None,
            claude_bin: "claude".into(),
            agent_program: Some(bin.clone()),
            claude_projects: home.join("projects"),
        };
        let app = AppState::new(Store::in_memory().await.unwrap(), config);
        let key = TaskKey::new("PROJ-1");
        let mut task = Task::new(key.clone(), "Card retries", "feature");
        task.chat_mode = chat;
        app.store.create_task(&task).await.unwrap();
        app.transition(&key, Event::Start).await.unwrap();
        app.transition(&key, Event::WorkspaceReady).await.unwrap();
        let ws = Workspace {
            path: home.join("ws"),
            branch: "feature/PROJ-1".into(),
            kind: delilah_core::ProjectKind::Repo,
            submodules: vec![],
        };
        app.store.save_workspace(&key, &home, &ws).await.unwrap();
        let record = SessionRecord {
            id: "main".into(),
            task: "PROJ-1".into(),
            kind: SessionKind::Claude,
            label: "Main".into(),
            primary: true,
            closed: false,
            read_only: false,
            conversation: false,
            step: None,
        };
        app.store.create_session(&record).await.unwrap();
        let (runtime, events): (delilah_agents::runtime::Runtime, _) = if chat {
            let (s, rx) =
                delilah_agents::stream::StreamSession::spawn(tokio::process::Command::new(&bin))
                    .unwrap();
            (Arc::new(s).into(), Some(rx))
        } else {
            let mut cmd = portable_pty::CommandBuilder::new(&bin);
            cmd.args(["x"]);
            (
                delilah_agents::pty::PtySession::spawn(cmd, 200, 24, None)
                    .unwrap()
                    .into(),
                None,
            )
        };
        drop(events);
        let live = app
            .sessions
            .insert("main", "PROJ-1", SessionKind::Claude, true, runtime);
        live.set_state(LiveState::Waiting);
        app
    }

    async fn post(app: &AppState, key: &str, body: Value) -> (StatusCode, Value) {
        let req = Request::builder()
            .method("POST")
            .uri(format!("/api/v1/tasks/{key}/runtime"))
            .header("content-type", "application/json")
            .body(Body::from(body.to_string()))
            .unwrap();
        let res = crate::api::router(app.clone()).oneshot(req).await.unwrap();
        let status = res.status();
        let bytes = http_body_util::BodyExt::collect(res.into_body())
            .await
            .unwrap()
            .to_bytes();
        (
            status,
            serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        )
    }

    async fn chat_flag(app: &AppState) -> bool {
        app.store
            .get_task(&TaskKey::new("PROJ-1"))
            .await
            .unwrap()
            .unwrap()
            .chat_mode
    }

    fn running_on_stream(app: &AppState) -> Option<bool> {
        app.sessions
            .get("main")
            .map(|l| l.runtime.as_stream().is_some())
    }

    #[tokio::test]
    async fn an_idle_chat_task_goes_to_the_terminal_and_back() {
        let app = task_in(true, "exec sleep 5").await;
        let mut rx = app.events.subscribe();
        let (status, _) = post(&app, "PROJ-1", json!({"mode": "terminal"})).await;
        assert_eq!(status, StatusCode::NO_CONTENT);
        assert!(!chat_flag(&app).await);
        assert_eq!(running_on_stream(&app), Some(false));
        let mut topics = Vec::new();
        while let Ok(t) = rx.try_recv() {
            topics.push(t);
        }
        assert!(topics.contains(&Topic::Tasks) && topics.contains(&Topic::Sessions));

        let (status, _) = post(&app, "PROJ-1", json!({"mode": "chat"})).await;
        assert_eq!(status, StatusCode::NO_CONTENT);
        assert!(chat_flag(&app).await);
        assert_eq!(running_on_stream(&app), Some(true));
        if let Some(l) = app.sessions.get("main") {
            let _ = l.runtime.kill();
        }
    }

    #[tokio::test]
    async fn a_working_session_or_a_pending_ask_answers_409() {
        let app = task_in(true, "exec sleep 5").await;
        app.sessions
            .get("main")
            .unwrap()
            .set_state(LiveState::Working);
        let (status, body) = post(&app, "PROJ-1", json!({"mode": "terminal"})).await;
        assert_eq!(status, StatusCode::CONFLICT);
        assert!(
            body.to_string().contains("wait for the turn to end"),
            "{body}"
        );
        assert!(chat_flag(&app).await);

        let live = app.sessions.get("main").unwrap();
        live.set_state(LiveState::Waiting);
        app.permissions.insert(crate::permissions::PermissionAsk {
            id: "p1".into(),
            request_id: "r1".into(),
            task: "PROJ-1".into(),
            session: "main".into(),
            tool: "Bash".into(),
            input: json!({}),
            suggestion: None,
            rule: None,
            summary: "ls".into(),
            at: chrono::Utc::now(),
        });
        let (status, _) = post(&app, "PROJ-1", json!({"mode": "terminal"})).await;
        assert_eq!(status, StatusCode::CONFLICT);
        assert!(chat_flag(&app).await);
        let _ = live.runtime.kill();
    }

    #[tokio::test]
    async fn the_same_mode_a_bad_mode_and_an_unknown_task() {
        let app = task_in(true, "exec sleep 5").await;
        let before = app.sessions.get("main").unwrap().runtime.clone();
        let (status, _) = post(&app, "PROJ-1", json!({"mode": "chat"})).await;
        assert_eq!(status, StatusCode::NO_CONTENT);
        assert!(app.sessions.get("main").unwrap().runtime.same(&before));
        let (status, _) = post(&app, "PROJ-1", json!({"mode": "voice"})).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        let (status, _) = post(&app, "NOPE-1", json!({"mode": "chat"})).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        let _ = before.kill();
    }

    #[tokio::test]
    async fn a_task_with_nothing_running_only_changes_its_flag() {
        let app = task_in(false, "exec sleep 5").await;
        let live = app.sessions.get("main").unwrap();
        let _ = live.runtime.kill();
        live.runtime.wait().await;
        app.sessions.remove_if_same("main", &live.runtime);
        let (status, _) = post(&app, "PROJ-1", json!({"mode": "chat"})).await;
        assert_eq!(status, StatusCode::NO_CONTENT);
        assert!(chat_flag(&app).await);
        assert!(app.sessions.get("main").is_none());
    }

    #[tokio::test]
    async fn a_chat_relaunch_that_fails_rolls_back_to_the_terminal() {
        let script = "case \"$*\" in *stream-json*) exit 1;; esac\nexec sleep 5";
        let app = task_in(false, script).await;
        let (status, _) = post(&app, "PROJ-1", json!({"mode": "chat"})).await;
        assert!(!status.is_success(), "{status}");
        assert!(!chat_flag(&app).await);
        assert_eq!(running_on_stream(&app), Some(false));
        if let Some(l) = app.sessions.get("main") {
            let _ = l.runtime.kill();
        }
    }
}
