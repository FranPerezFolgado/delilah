//! `POST /api/v1/tasks/{key}/runtime`: switches a task between chat mode and the
//! terminal. Its live Claude sessions are stopped and started again with `--resume` in
//! the other runtime, so the conversation goes on.

use std::time::Duration;

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use delilah_agents::runtime::RuntimeKind;
use delilah_core::{Event, TaskKey, TaskView};
use serde::Deserialize;

use crate::api::ApiError;
use crate::events::Topic;
use crate::run::launch_session_settled;
use crate::sessions::{Live, LiveState};
use crate::state::AppState;
use crate::store::StoreError;
use crate::store_sessions::{EntryKind, SessionKind, SessionRecord};

/// How long a chat agent relaunched without a prompt must stay alive for the switch to
/// count as done: the real CLI says nothing until it gets a message. A deliberate user
/// action, so it can take a moment.
const SWITCH_SETTLE: Duration = Duration::from_secs(3);

const BUSY: &str = "wait for the turn to end or press Detener";

/// Background commands or sub-agents of a session would die with the switch.
fn background_refusal(app: &AppState) -> ApiError {
    ApiError::conflict(app.lang().pick(
        "Hay trabajo en segundo plano: espera a que termine o pulsa Detener",
        "Background work is running: wait for it to finish or press Stop",
    ))
}

#[derive(Debug, Deserialize)]
pub struct SetRuntime {
    pub mode: String,
}

/// `POST /sessions/{id}/runtime`: switches one session between chat and terminal.
pub async fn set_session_runtime(
    State(app): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<SetRuntime>,
) -> Result<StatusCode, ApiError> {
    set_session_runtime_with(app, id, body, SWITCH_SETTLE).await
}

/// "chat" is the stream runtime and "terminal" the PTY, in every route.
fn parse_mode(mode: &str) -> Result<RuntimeKind, ApiError> {
    match TaskView::parse(mode) {
        Some(TaskView::Chat) => Ok(RuntimeKind::Stream),
        Some(_) => Ok(RuntimeKind::Pty),
        None => Err(ApiError::bad_request(format!(
            "unknown mode {mode:?}: use \"chat\" or \"terminal\""
        ))),
    }
}

fn mode_word(kind: RuntimeKind) -> &'static str {
    match kind {
        RuntimeKind::Stream => "chat",
        RuntimeKind::Pty => "terminal",
    }
}

/// History only: the status of the task is not touched.
async fn record_switch(app: &AppState, key: &str, session: Option<&str>, kind: RuntimeKind) {
    let event = Event::RuntimeSwitched {
        session: session.map(str::to_string),
        mode: mode_word(kind).into(),
    };
    if let Err(e) = app.transition(&TaskKey::new(key), event).await {
        tracing::warn!(task = %key, "cannot record the runtime switch: {e}");
    }
}

async fn set_session_runtime_with(
    app: AppState,
    id: String,
    body: SetRuntime,
    settle: Duration,
) -> Result<StatusCode, ApiError> {
    let target = parse_mode(&body.mode)?;
    let record = app
        .store
        .session(&id)
        .await?
        .filter(|r| !r.closed)
        .ok_or_else(|| ApiError::not_found(format!("session {id} does not exist or was closed")))?;
    // Shells, setups, ideas and a project's sessions have no runtime to choose.
    if record.kind != SessionKind::Claude
        || record.conversation
        || crate::store_sessions::project_of(&record.task).is_some()
    {
        return Err(ApiError::conflict(
            "only a task's Claude sessions switch runtime",
        ));
    }
    let live = app.sessions.get(&id).filter(|l| !l.runtime.has_exited());
    let current = match &live {
        Some(l) if l.runtime.as_stream().is_some() => RuntimeKind::Stream,
        Some(_) => RuntimeKind::Pty,
        None => crate::run::runtime_kind(&app, &record).await?,
    };
    if current == target {
        return Err(ApiError::conflict(format!(
            "the session already runs in {}",
            mode_word(target)
        )));
    }
    let previous = app.store.session_extras(&id).await?.runtime_override;

    // Held from the busy check to the end of the relaunch, as in the task route.
    let _typing = match &live {
        Some(l) => Some(l.typing.clone().lock_owned().await),
        None => None,
    };
    if let Some(l) = &live
        && is_busy(&app, l).await?
    {
        return Err(ApiError::conflict(BUSY));
    }
    if let Some(l) = &live
        && l.progress().has_background()
    {
        return Err(background_refusal(&app));
    }

    app.store.set_runtime_override(&id, Some(target)).await?;
    if live.is_some() {
        let outcome = relaunch_all(
            &app,
            &record.task,
            std::slice::from_ref(&record),
            settle,
            target == RuntimeKind::Pty,
        )
        .await;
        if let Err(e) = outcome {
            // Back to how it was.
            if let Err(s) = app.store.set_runtime_override(&id, previous).await {
                tracing::warn!(session = %id, "cannot put the runtime back: {s}");
            }
            let back = relaunch_all(
                &app,
                &record.task,
                std::slice::from_ref(&record),
                settle,
                target != RuntimeKind::Pty,
            )
            .await;
            if let Err(back) = back {
                tracing::warn!(session = %id, "cannot restore the session: {}", back.message());
            }
            app.emit(Topic::Sessions);
            return Err(e);
        }
    }
    record_switch(&app, &record.task, Some(&id), target).await;
    app.emit(Topic::Sessions);
    app.emit(Topic::Tasks);
    Ok(StatusCode::NO_CONTENT)
}

pub async fn set_runtime(
    State(app): State<AppState>,
    Path(key): Path<String>,
    Json(body): Json<SetRuntime>,
) -> Result<StatusCode, ApiError> {
    set_runtime_with(app, key, body, SWITCH_SETTLE).await
}

async fn set_runtime_with(
    app: AppState,
    key: String,
    body: SetRuntime,
    settle: Duration,
) -> Result<StatusCode, ApiError> {
    // The same words as the task's view; the runtime is `chat_mode`.
    let chat = parse_mode(&body.mode)? == RuntimeKind::Stream;
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
    // The sessions that have a runtime of their own, to put back if the switch fails.
    let overrides = {
        let mut v = Vec::new();
        for r in app.store.open_sessions(Some(&key)).await? {
            let o = app.store.session_extras(&r.id).await?.runtime_override;
            if o.is_some() {
                v.push((r.id, o));
            }
        }
        v
    };
    if task.chat_mode == chat
        && live
            .iter()
            .all(|(l, _)| l.runtime.as_stream().is_some() == chat)
    {
        // Nothing to switch, but the task's runtime is the one for all of its sessions.
        if !overrides.is_empty() {
            app.store.clear_runtime_overrides(&key).await?;
            app.emit(Topic::Sessions);
        }
        return Ok(StatusCode::NO_CONTENT);
    }
    // Held from the busy check to the end of the relaunch, so a message cannot be typed
    // in between (POST .../messages takes the same lock). In session-id order, so two
    // switches never wait on each other.
    live.sort_by(|a, b| a.0.id.cmp(&b.0.id));
    let mut _typing = Vec::with_capacity(live.len());
    for (l, _) in &live {
        _typing.push(l.typing.clone().lock_owned().await);
    }
    for (l, _) in &live {
        if is_busy(&app, l).await? {
            return Err(ApiError::conflict(BUSY));
        }
        if l.progress().has_background() {
            return Err(background_refusal(&app));
        }
    }

    let previous = task.chat_mode;
    app.store.set_chat_mode(&task_key, chat).await?;
    // The task's runtime now rules every session: their own choices are cleared.
    app.store.clear_runtime_overrides(&key).await?;
    app.emit(Topic::Tasks);
    let sessions: Vec<SessionRecord> = live.into_iter().map(|(_, r)| r).collect();
    if sessions.is_empty() {
        record_switch(&app, &key, None, target_kind(chat)).await;
        app.emit(Topic::Sessions);
        return Ok(StatusCode::NO_CONTENT);
    }
    let outcome = relaunch_all(&app, &key, &sessions, settle, !chat).await;
    if let Err(e) = outcome {
        // Back to how it was: the flag and the overrides first, then every session as it ran.
        if let Err(s) = app.store.set_chat_mode(&task_key, previous).await {
            tracing::warn!(task = %key, "cannot put chat mode back: {s}");
        }
        for (id, o) in &overrides {
            if let Err(s) = app.store.set_runtime_override(id, *o).await {
                tracing::warn!(session = %id, "cannot put the runtime back: {s}");
            }
        }
        if let Err(back) = relaunch_all(&app, &key, &sessions, settle, chat).await {
            tracing::warn!(task = %key, "cannot restore the sessions: {}", back.message());
        }
        app.emit(Topic::Tasks);
        app.emit(Topic::Sessions);
        return Err(e);
    }
    record_switch(&app, &key, None, target_kind(chat)).await;
    app.emit(Topic::Sessions);
    Ok(StatusCode::NO_CONTENT)
}

fn target_kind(chat: bool) -> RuntimeKind {
    if chat {
        RuntimeKind::Stream
    } else {
        RuntimeKind::Pty
    }
}

/// Whether a switch would cut a turn: it is working, a card waits for an answer, or it
/// is still starting with a message sent to it and not yet delivered (its first turn is
/// on the way). A resumed session with no message sitting in `Starting` is not busy.
async fn is_busy(app: &AppState, l: &Live) -> Result<bool, ApiError> {
    let state = l.state();
    if state == LiveState::Working
        || app.asks.has_session(&l.id)
        || app.permissions.has_session(&l.id)
    {
        return Ok(true);
    }
    if state != LiveState::Starting {
        return Ok(false);
    }
    Ok(app
        .store
        .entries(&l.id)
        .await?
        .iter()
        .any(|e| e.kind == EntryKind::User && e.body["state"] == "sent"))
}

/// Stops each session and starts it again with `--resume`, in the runtime the task's
/// flag (or the session's own override) says now. Stops at the first that fails to come up.
async fn relaunch_all(
    app: &AppState,
    key: &str,
    sessions: &[SessionRecord],
    settle: Duration,
    to_terminal: bool,
) -> Result<(), ApiError> {
    let ws = crate::owners::workspace(app, key)
        .await
        .map_err(|_| ApiError::conflict(format!("{key} has no workspace (archived?)")))?;
    for record in sessions {
        // Whatever is running now (the original, or what a failed attempt left).
        if let Some(current) = app.sessions.get(&record.id) {
            // Open cards end with the outcome `terminal` when it goes to the terminal.
            if to_terminal {
                crate::permissions::release_session_terminal(app, &record.id).await;
            } else {
                crate::permissions::release_session(app, &record.id).await;
            }
            let _ = current.runtime.kill();
            current.runtime.wait().await;
            app.sessions.remove_if_same(&record.id, &current.runtime);
        }
        launch_session_settled(app, record, &ws.path, true, settle).await?;
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
            ..Config::test_defaults()
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

    /// The switch itself, with a settle of the test's choosing.
    async fn switch(app: &AppState, mode: &str, settle: Duration) -> (StatusCode, String) {
        let body = SetRuntime { mode: mode.into() };
        match set_runtime_with(app.clone(), "PROJ-1".into(), body, settle).await {
            Ok(s) => (s, String::new()),
            Err(e) => (e.status, e.message),
        }
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

    /// A second live Claude session `second` of PROJ-1, on the same runtime as `main`.
    async fn add_second(app: &AppState, chat: bool) {
        let record = SessionRecord {
            id: "second".into(),
            label: "Second".into(),
            primary: false,
            ..app.store.session("main").await.unwrap().unwrap()
        };
        app.store.create_session(&record).await.unwrap();
        let bin = app.config.agent_program.clone().unwrap();
        let runtime: delilah_agents::runtime::Runtime = if chat {
            let (s, rx) =
                delilah_agents::stream::StreamSession::spawn(tokio::process::Command::new(&bin))
                    .unwrap();
            drop(rx);
            Arc::new(s).into()
        } else {
            let mut cmd = portable_pty::CommandBuilder::new(&bin);
            cmd.args(["x"]);
            delilah_agents::pty::PtySession::spawn(cmd, 200, 24, None)
                .unwrap()
                .into()
        };
        let live = app
            .sessions
            .insert("second", "PROJ-1", SessionKind::Claude, false, runtime);
        live.set_state(LiveState::Waiting);
    }

    async fn switch_session(
        app: &AppState,
        id: &str,
        mode: &str,
        settle: Duration,
    ) -> (StatusCode, String) {
        let body = SetRuntime { mode: mode.into() };
        match set_session_runtime_with(app.clone(), id.into(), body, settle).await {
            Ok(s) => (s, String::new()),
            Err(e) => (e.status, e.message),
        }
    }

    async fn switched_events(app: &AppState) -> Vec<(Option<String>, String)> {
        app.store
            .events(&TaskKey::new("PROJ-1"))
            .await
            .unwrap()
            .into_iter()
            .filter_map(|r| match r.event {
                Event::RuntimeSwitched { session, mode } => Some((session, mode)),
                _ => None,
            })
            .collect()
    }

    fn kill_all(app: &AppState) {
        for l in app.sessions.of_task("PROJ-1") {
            let _ = l.runtime.kill();
        }
    }

    #[tokio::test]
    async fn switching_one_session_leaves_the_others() {
        let app = task_in(true, "exec sleep 5").await;
        add_second(&app, true).await;
        let mut rx = app.events.subscribe();
        let (status, m) =
            switch_session(&app, "main", "terminal", Duration::from_millis(500)).await;
        assert_eq!(status, StatusCode::NO_CONTENT, "{m}");
        assert_eq!(running_on_stream(&app), Some(false));
        let second = app.sessions.get("second").unwrap();
        assert!(second.runtime.as_stream().is_some());
        // The task keeps its mode; only the session has a choice of its own.
        assert!(chat_flag(&app).await);
        let main = app.store.session("main").await.unwrap().unwrap();
        assert_eq!(
            app.store
                .session_extras("main")
                .await
                .unwrap()
                .runtime_override,
            Some(RuntimeKind::Pty)
        );
        assert_eq!(
            app.store
                .session_extras("second")
                .await
                .unwrap()
                .runtime_override,
            None
        );
        assert_eq!(
            crate::run::runtime_kind(&app, &main).await.ok().unwrap(),
            RuntimeKind::Pty
        );
        let mut topics = Vec::new();
        while let Ok(t) = rx.try_recv() {
            topics.push(t);
        }
        assert!(topics.contains(&Topic::Tasks) && topics.contains(&Topic::Sessions));
        kill_all(&app);
    }

    #[tokio::test]
    async fn the_override_survives_a_restart() {
        let app = task_in(true, "exec sleep 5").await;
        let (status, _) =
            switch_session(&app, "main", "terminal", Duration::from_millis(500)).await;
        assert_eq!(status, StatusCode::NO_CONTENT);
        // A restart: nothing runs, the launch reads the stored choice.
        kill_all(&app);
        let live = app.sessions.get("main").unwrap();
        live.runtime.wait().await;
        app.sessions.remove_if_same("main", &live.runtime);
        let main = app.store.session("main").await.unwrap().unwrap();
        assert_eq!(
            crate::run::runtime_kind(&app, &main).await.ok().unwrap(),
            RuntimeKind::Pty
        );
        assert!(chat_flag(&app).await);
        // And switching back is a change again, not "already there".
        let (status, _) = switch_session(&app, "main", "chat", Duration::from_millis(500)).await;
        assert_eq!(status, StatusCode::NO_CONTENT);
        kill_all(&app);
    }

    #[tokio::test]
    async fn a_running_turn_is_409() {
        let app = task_in(true, "exec sleep 5").await;
        app.sessions
            .get("main")
            .unwrap()
            .set_state(LiveState::Working);
        let (status, m) =
            switch_session(&app, "main", "terminal", Duration::from_millis(500)).await;
        assert_eq!(status, StatusCode::CONFLICT);
        assert!(m.contains("wait for the turn to end"), "{m}");
        assert_eq!(
            app.store
                .session_extras("main")
                .await
                .unwrap()
                .runtime_override,
            None
        );
        assert_eq!(running_on_stream(&app), Some(true));
        assert!(switched_events(&app).await.is_empty());
        kill_all(&app);
    }

    fn start_background(app: &AppState, id: &str) {
        use delilah_agents::stream::{StreamEvent, TaskEvent, TaskKind};
        app.sessions.get(id).unwrap().update_progress(|p| {
            crate::progress::apply(
                p,
                &StreamEvent::Task(TaskEvent::Started {
                    task_id: "bg1".into(),
                    tool_use_id: None,
                    kind: TaskKind::Command,
                    description: "sleep".into(),
                    background: true,
                }),
                chrono::Utc::now(),
            )
        });
    }

    #[tokio::test]
    async fn background_work_refuses_the_task_switch() {
        let app = task_in(true, "exec sleep 5").await;
        start_background(&app, "main");
        assert!(
            app.sessions
                .get("main")
                .unwrap()
                .progress()
                .has_background()
        );
        let (status, m) = switch(&app, "terminal", Duration::from_millis(500)).await;
        assert_eq!(status, StatusCode::CONFLICT);
        assert!(m.contains("espera a que termine o pulsa Detener"), "{m}");
        assert!(chat_flag(&app).await);
        assert_eq!(running_on_stream(&app), Some(true));
        assert!(switched_events(&app).await.is_empty());
        app.lang.set(crate::lang::Lang::En);
        let (_, m) = switch(&app, "terminal", Duration::from_millis(500)).await;
        assert!(m.contains("Background work is running"), "{m}");
        kill_all(&app);
    }

    #[tokio::test]
    async fn background_work_refuses_the_session_switch() {
        let app = task_in(true, "exec sleep 5").await;
        start_background(&app, "main");
        let (status, m) =
            switch_session(&app, "main", "terminal", Duration::from_millis(500)).await;
        assert_eq!(status, StatusCode::CONFLICT);
        assert!(m.contains("segundo plano"), "{m}");
        assert_eq!(running_on_stream(&app), Some(true));
        kill_all(&app);
    }

    #[tokio::test]
    async fn a_shell_session_is_409() {
        let app = task_in(true, "exec sleep 5").await;
        let shell = SessionRecord {
            id: "sh".into(),
            kind: SessionKind::Shell,
            label: "sh".into(),
            primary: false,
            ..app.store.session("main").await.unwrap().unwrap()
        };
        app.store.create_session(&shell).await.unwrap();
        let (status, _) = switch_session(&app, "sh", "terminal", Duration::from_millis(500)).await;
        assert_eq!(status, StatusCode::CONFLICT);
        kill_all(&app);
    }

    #[tokio::test]
    async fn the_session_route_answers_400_404_and_409_for_the_same_runtime() {
        let app = task_in(true, "exec sleep 5").await;
        let r = |mode: &str| SetRuntime { mode: mode.into() };
        let err = |res: Result<StatusCode, ApiError>| res.unwrap_err().status;
        assert_eq!(
            err(
                set_session_runtime_with(app.clone(), "main".into(), r("voice"), SWITCH_SETTLE)
                    .await
            ),
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            err(
                set_session_runtime_with(app.clone(), "nope".into(), r("chat"), SWITCH_SETTLE)
                    .await
            ),
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            err(
                set_session_runtime_with(app.clone(), "main".into(), r("chat"), SWITCH_SETTLE)
                    .await
            ),
            StatusCode::CONFLICT
        );
        kill_all(&app);
    }

    #[tokio::test]
    async fn both_routes_record_runtime_switched() {
        let app = task_in(true, "exec sleep 5").await;
        let (status, _) =
            switch_session(&app, "main", "terminal", Duration::from_millis(500)).await;
        assert_eq!(status, StatusCode::NO_CONTENT);
        let (status, _) = switch(&app, "terminal", Duration::from_millis(500)).await;
        assert_eq!(status, StatusCode::NO_CONTENT);
        assert_eq!(
            switched_events(&app).await,
            vec![
                (Some("main".to_string()), "terminal".to_string()),
                (None, "terminal".to_string()),
            ]
        );
        // The status is untouched by either.
        let task = app
            .store
            .get_task(&TaskKey::new("PROJ-1"))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(task.status, delilah_core::TaskStatus::Running);
        kill_all(&app);
    }

    #[tokio::test]
    async fn the_task_route_clears_every_session_override() {
        let app = task_in(true, "exec sleep 5").await;
        add_second(&app, true).await;
        let (status, _) =
            switch_session(&app, "main", "terminal", Duration::from_millis(500)).await;
        assert_eq!(status, StatusCode::NO_CONTENT);
        assert!(
            app.store
                .session_extras("main")
                .await
                .unwrap()
                .runtime_override
                .is_some()
        );

        // The task goes to the terminal: every session follows, none keeps a choice.
        let (status, _) = post(&app, "PROJ-1", json!({"mode": "terminal"})).await;
        assert_eq!(status, StatusCode::NO_CONTENT);
        for id in ["main", "second"] {
            assert_eq!(
                app.store.session_extras(id).await.unwrap().runtime_override,
                None,
                "{id}"
            );
            assert_eq!(
                app.sessions
                    .get(id)
                    .map(|l| l.runtime.as_stream().is_some()),
                Some(false),
                "{id}"
            );
        }
        assert!(!chat_flag(&app).await);
        kill_all(&app);
    }

    #[tokio::test]
    async fn an_idle_chat_task_goes_to_the_terminal_and_back() {
        let app = task_in(true, "exec sleep 5").await;
        let mut rx = app.events.subscribe();
        let (status, _) = switch(&app, "terminal", Duration::from_millis(500)).await;
        assert_eq!(status, StatusCode::NO_CONTENT);
        assert!(!chat_flag(&app).await);
        assert_eq!(running_on_stream(&app), Some(false));
        let mut topics = Vec::new();
        while let Ok(t) = rx.try_recv() {
            topics.push(t);
        }
        assert!(topics.contains(&Topic::Tasks) && topics.contains(&Topic::Sessions));

        let mut rx = app.events.subscribe();
        let (status, _) = switch(&app, "chat", Duration::from_millis(500)).await;
        assert_eq!(status, StatusCode::NO_CONTENT);
        assert!(chat_flag(&app).await);
        assert_eq!(running_on_stream(&app), Some(true));
        let mut topics = Vec::new();
        while let Ok(t) = rx.try_recv() {
            topics.push(t);
        }
        assert!(topics.contains(&Topic::Tasks) && topics.contains(&Topic::Sessions));
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
    async fn a_pending_ask_question_answers_409() {
        let app = task_in(true, "exec sleep 5").await;
        let hook = {
            let app = app.clone();
            tokio::spawn(async move {
                let body = json!({
                    "tool_name": "AskUserQuestion",
                    "tool_input": {"questions": [{"question": "Which?", "header": "H",
                        "options": [{"label": "A", "description": "a"}], "multiSelect": false}]},
                });
                let req = Request::builder()
                    .method("POST")
                    .uri("/hooks/claude/ask?task=PROJ-1&session=main")
                    .header("content-type", "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap();
                let _ = crate::api::router(app).oneshot(req).await;
            })
        };
        for _ in 0..200 {
            if app.asks.has_session("main") {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert!(app.asks.has_session("main"));
        let (status, _) = post(&app, "PROJ-1", json!({"mode": "terminal"})).await;
        assert_eq!(status, StatusCode::CONFLICT);
        assert!(chat_flag(&app).await);
        hook.abort();
        if let Some(l) = app.sessions.get("main") {
            let _ = l.runtime.kill();
        }
    }

    #[tokio::test]
    async fn a_starting_session_is_busy_only_with_a_message_on_its_way() {
        let app = task_in(true, "exec sleep 5").await;
        let live = app.sessions.get("main").unwrap();
        // Resumed with no prompt: still Starting, but nothing to cut.
        live.set_state(LiveState::Starting);
        let (status, _) = switch(&app, "terminal", Duration::from_millis(500)).await;
        assert_eq!(status, StatusCode::NO_CONTENT);

        // A first message sent and not yet delivered: its turn is on the way.
        let app = task_in(true, "exec sleep 5").await;
        let live = app.sessions.get("main").unwrap();
        live.set_state(LiveState::Starting);
        app.store
            .add_entry(
                "main",
                EntryKind::User,
                &json!({"text": "hi", "state": "sent", "from": "view"}),
            )
            .await
            .unwrap();
        let (status, _) = switch(&app, "terminal", Duration::from_millis(500)).await;
        assert_eq!(status, StatusCode::CONFLICT);
        assert!(chat_flag(&app).await);
        let _ = live.runtime.kill();
    }

    #[tokio::test]
    async fn the_switch_waits_for_a_message_being_typed() {
        let app = task_in(true, "exec sleep 5").await;
        let live = app.sessions.get("main").unwrap();
        let guard = live.typing.clone().lock_owned().await;
        let a = app.clone();
        let switching =
            tokio::spawn(async move { switch(&a, "terminal", Duration::from_millis(500)).await });
        tokio::time::sleep(Duration::from_millis(200)).await;
        // Held by the send: nothing was switched meanwhile.
        assert!(!switching.is_finished());
        assert!(chat_flag(&app).await);
        // The send lands (the session is now working): the switch sees it and refuses.
        live.set_state(LiveState::Working);
        drop(guard);
        let (status, _) = switching.await.unwrap();
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
        // A settle far longer than the agent takes to die: it is seen dying, never taken as up.
        let (status, message) = switch(&app, "chat", Duration::from_secs(30)).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{message}");
        assert!(
            message.starts_with("chat mode could not start"),
            "{message}"
        );
        assert!(!chat_flag(&app).await);
        assert_eq!(running_on_stream(&app), Some(false));
        if let Some(l) = app.sessions.get("main") {
            let _ = l.runtime.kill();
        }
    }
}
