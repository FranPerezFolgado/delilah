//! Endpoints the agent's hooks and status line post to (see `claude::settings`).

use axum::Json;
use axum::extract::{Query, State};
use axum::http::StatusCode;
use chrono::Utc;
use delilah_agents::claude::{self, Signal};
use delilah_agents::limits;
use delilah_core::{Event, TaskKey};
use serde::Deserialize;
use serde_json::Value;
use tokio::io::AsyncWriteExt;

use crate::events::Topic;
use crate::inbox::{self, Change};
use crate::sessions::LiveState;
use crate::state::{AppState, UsageReport};
use crate::store::StoreError;

#[derive(Deserialize)]
pub struct HookQuery {
    pub(crate) task: String,
    /// Absent in settings written before sessions existed: the primary one.
    pub(crate) session: Option<String>,
}

pub async fn claude_hook(
    State(app): State<AppState>,
    Query(q): Query<HookQuery>,
    Json(hook): Json<Value>,
) -> StatusCode {
    apply_hook(app, q, hook).await
}

/// What a chat-mode agent's stream says when it hits the usage limit, as the `StopFailure`
/// hook would say it (which print mode was never seen to send): the one place a limit
/// pauses a task, whichever of the two reports it first. A second report finds the task
/// already paused and changes nothing.
pub(crate) async fn limit_hit_from_stream(app: &AppState, task: &str, session: &str) {
    let hook = serde_json::json!({
        "hook_event_name": "StopFailure",
        "error": "rate_limit",
        "source": "stream",
    });
    // Not logged raw: hooks.jsonl holds what the agent really posted, not what the
    // daemon made up from the stream.
    apply_signal(app.clone(), task, Some(session), Signal::TurnFailed, &hook).await;
}

async fn apply_hook(app: AppState, q: HookQuery, hook: Value) -> StatusCode {
    if !is_owner_name(&q.task) {
        return StatusCode::BAD_REQUEST;
    }
    // Kept raw: the exact payloads (e.g. a real StopFailure) are what the next
    // iteration of the adapter is built from.
    // PreToolUse is not logged: it fires for every tool with the full tool_input
    // (e.g. Write contents), which PostToolUse already carries; logging both
    // would double the file.
    if hook["hook_event_name"].as_str() != Some("PreToolUse")
        && let Err(e) = append_raw(&app, &q.task, &hook).await
    {
        tracing::warn!(task = %q.task, "cannot log hook: {e}");
    }
    let signal = claude::classify(&hook);
    apply_signal(app, &q.task, q.session.as_deref(), signal, &hook).await;
    StatusCode::NO_CONTENT
}

/// What a hook (or a stream event that stands for one) does to the session, the inbox and
/// the task, without logging it.
async fn apply_signal(
    app: AppState,
    task: &str,
    session: Option<&str>,
    signal: Signal,
    hook: &Value,
) {
    let q = HookQuery {
        task: task.to_string(),
        session: session.map(str::to_string),
    };
    let hook = hook.clone();
    let live = match &q.session {
        Some(id) => app.sessions.get(id),
        None => app.sessions.primary(&q.task),
    };
    // Only a path that passed the check replaces the current one.
    if let Some(live) = &live
        && let Some(path) = trusted_transcript(&app, &hook)
    {
        live.set_transcript(path);
    }
    if let Some(live) = &live {
        crate::transcript_watch::poke(&app, &live.id);
    }
    // No live session (e.g. a hook from before a restart): treat it as the primary.
    let primary = live.as_ref().is_none_or(|l| l.primary);
    if signal == Signal::SessionStarted
        && let Some(live) = &live
    {
        live.started.notify_one();
    }

    let limit_reset = match signal {
        Signal::TurnEnded | Signal::TurnFailed => {
            let limits = app.latest_limits();
            limits::limit_reset(signal, &hook, limits.as_ref(), Utc::now())
        }
        _ => None,
    };
    if let Some(live) = &live {
        live.set_state(match (signal, limit_reset) {
            (_, Some(_)) => LiveState::Paused,
            (Signal::PromptSubmitted | Signal::Working, _) => LiveState::Working,
            (Signal::NeedsUser | Signal::TurnEnded | Signal::TurnFailed, _) => LiveState::Waiting,
            _ => live.state(),
        });
    }

    // A Claude session's conversation keeps its own state (an idea, its whole history).
    let session_id = q
        .session
        .clone()
        .or_else(|| live.as_ref().map(|l| l.id.clone()));
    if let Some(id) = &session_id {
        crate::conversation::on_hook(&app, id, live.as_ref(), signal, &hook).await;
        note_activity(&app, id, signal, &hook).await;
    }

    if let Some(live) = &live
        && let Some(event) = claude::classify_activity(&hook)
    {
        live.update_activity(event);
    }

    // What the session now waits on you for, for the inbox.
    if let Some(id) = q
        .session
        .clone()
        .or_else(|| live.as_ref().map(|l| l.id.clone()))
    {
        let current = app.store.attention(&id).await.ok().flatten();
        let change = inbox::attention_change(
            signal,
            &hook,
            limit_reset.is_some(),
            current.as_ref(),
            Utc::now(),
            app.lang(),
        );
        crate::autopilot::on_attention(&app, &TaskKey::new(&q.task), &id, &change).await;
        let changes = match &change {
            Change::Keep => false,
            Change::Clear => current.is_some(),
            Change::Set(_) => true,
        };
        let saved = match change {
            Change::Keep => Ok(()),
            Change::Clear if current.is_none() => Ok(()),
            Change::Clear => app.store.set_attention(&id, None).await,
            Change::Set(attention) => app.store.set_attention(&id, Some(&attention)).await,
        };
        if changes {
            app.emit(Topic::Inbox);
        }
        if let Err(e) = saved {
            tracing::warn!(session = %id, "cannot record what the session waits for: {e}");
        }
    }

    if signal == Signal::TurnEnded && limit_reset.is_none() {
        note_turn_done(&app, &q.task, session_id.as_deref(), &hook).await;
    }

    let event = match (signal, limit_reset) {
        // The subscription is shared: a limit hit by any session pauses the task.
        (_, Some(resume_at)) => {
            tracing::info!(task = %q.task, %resume_at, "turn ended on the usage limit");
            Some(Event::LimitReached { resume_at })
        }
        // Other turns only move the task when they are the primary session's.
        _ if !primary => None,
        (Signal::PromptSubmitted | Signal::Working, _) => Some(Event::UserAnswered),
        (Signal::NeedsUser | Signal::TurnEnded | Signal::TurnFailed, _) => Some(Event::AgentAsked),
        _ => None,
    };
    // What the agent did may move the task's phase (a task's sessions only).
    if crate::store_sessions::project_of(&q.task).is_none() {
        crate::phases::on_hook(&app, &q.task, signal, &hook, primary, limit_reset.is_some()).await;
    }

    // A project's session has no task to move: only the session and the inbox change.
    let event = event.filter(|_| crate::store_sessions::project_of(&q.task).is_none());
    if let Some(event) = event {
        match app.transition(&TaskKey::new(&q.task), event).await {
            // Hooks repeat and overlap (a Notification after Stop, the first prompt
            // while already running): a transition that does not apply is not an error.
            Ok(_) | Err(StoreError::Transition(_)) => {}
            Err(e) => tracing::warn!(task = %q.task, "hook not applied: {e}"),
        }
    }
}

/// A finished turn or a submitted prompt is the session's last activity; Claude's closing
/// message (or the prompt) is what its row shows.
async fn note_activity(app: &AppState, session: &str, signal: Signal, hook: &Value) {
    let message = match signal {
        Signal::TurnEnded => hook["last_assistant_message"].as_str(),
        Signal::PromptSubmitted => hook["prompt"].as_str(),
        _ => return,
    };
    match app.store.touch_session(session, message).await {
        Ok(()) => app.emit(Topic::Sessions),
        Err(e) => tracing::warn!(session, "cannot record the session's activity: {e}"),
    }
}

/// A non-primary Claude session ended a turn: an aviso (`turn-done`) for the inbox. The
/// primary session's turn is the `waiting` of the task instead. The notice is a row of
/// `pr_notices` on purpose; its cause is the session and the moment, one per turn.
async fn note_turn_done(app: &AppState, task: &str, session: Option<&str>, hook: &Value) {
    use crate::store_prs::{KIND_TURN_DONE, NewNotice};
    use crate::store_sessions::SessionKind;
    if crate::store_sessions::project_of(task).is_some() {
        return;
    }
    let Some(record) = (match session {
        Some(id) => app.store.session(id).await.ok().flatten(),
        None => None,
    }) else {
        return;
    };
    if record.primary || record.closed || record.kind != SessionKind::Claude {
        return;
    }
    let text = hook["last_assistant_message"]
        .as_str()
        .map(inbox::clip)
        .filter(|t| !t.is_empty())
        .unwrap_or_else(|| {
            app.lang()
                .pick(
                    "La sesión terminó su turno.",
                    "The session finished its turn.",
                )
                .into()
        });
    let notice = NewNotice {
        repo: ".".into(),
        kind: KIND_TURN_DONE.into(),
        cause: format!("{}:{}", record.id, Utc::now().timestamp_millis()),
        text,
        detail: Some(serde_json::json!({ "session": record.id })),
    };
    match app.store.insert_notice(&TaskKey::new(task), &notice).await {
        Ok(true) => app.emit(Topic::Inbox),
        Ok(false) => {}
        Err(e) => tracing::warn!(task, "cannot record the finished turn: {e}"),
    }
}

/// The transcript path a hook carries, only when it is absolute, ends in `.jsonl` and,
/// once symlinks are resolved, is inside Claude's projects folder: it comes from the
/// hook body, so it is not followed blindly. The path returned is the resolved one.
fn trusted_transcript(app: &AppState, hook: &Value) -> Option<std::path::PathBuf> {
    trusted_transcript_in(&app.config.claude_projects, hook)
}

fn trusted_transcript_in(root: &std::path::Path, hook: &Value) -> Option<std::path::PathBuf> {
    let path = std::path::Path::new(hook.get("transcript_path")?.as_str()?);
    let plain = path.is_absolute()
        && path.extension().is_some_and(|e| e == "jsonl")
        && !path
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir));
    if !plain {
        return None;
    }
    let resolved = delilah_agents::transcript::within(root, path)?;
    resolved.is_file().then_some(resolved)
}

/// Keeps the latest usage windows. A reading is sent every few seconds: only a new one
/// is news. Shared by the status line hook (terminals) and the stream's rate-limit events.
pub(crate) fn record_limits(app: &AppState, task: String, limits: Value) {
    let changed = {
        let mut usage = app.usage.lock().unwrap();
        let changed = usage.as_ref().is_none_or(|u| u.rate_limits != limits);
        *usage = Some(UsageReport {
            at: Utc::now(),
            task,
            rate_limits: limits,
        });
        changed
    };
    if changed {
        app.emit(Topic::Usage);
    }
}

pub async fn claude_statusline(
    State(app): State<AppState>,
    Query(q): Query<HookQuery>,
    Json(payload): Json<Value>,
) -> StatusCode {
    if let Some(limits) = payload.get("rate_limits").filter(|v| v.is_object()) {
        record_limits(&app, q.task, limits.clone());
    }
    StatusCode::NO_CONTENT
}

/// `POST /hooks/claude/guard?task=…&session=…`: the PreToolUse hook of a task without a
/// branch. Answers at once unless the tool would land work (see `branching::guard`); an
/// empty reply lets it through.
pub async fn guard_hook(
    State(app): State<AppState>,
    Query(q): Query<HookQuery>,
    Json(hook): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    if !is_owner_name(&q.task) {
        return Err(StatusCode::BAD_REQUEST);
    }
    let denial = crate::branching::guard(&app, &delilah_core::TaskKey::new(&q.task), &hook).await;
    Ok(Json(crate::branching::guard_reply(denial)))
}

/// A task key or `@<project>`: one plain folder name under the task dirs, so a forged
/// `?task=../..` cannot write elsewhere.
fn is_owner_name(owner: &str) -> bool {
    let mut parts = std::path::Path::new(owner).components();
    matches!(
        (parts.next(), parts.next()),
        (Some(std::path::Component::Normal(_)), None)
    )
}

async fn append_raw(app: &AppState, task: &str, hook: &Value) -> std::io::Result<()> {
    let dir = app.config.task_dir(task);
    tokio::fs::create_dir_all(&dir).await?;
    let mut file = tokio::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join("hooks.jsonl"))
        .await?;
    let mut line = serde_json::to_vec(hook)?;
    line.push(b'\n');
    file.write_all(&line).await
}

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use axum::http::Request;
    use delilah_core::{Task, TaskStatus};
    use serde_json::json;
    use tower::ServiceExt;

    use super::*;
    use crate::config::Config;
    use crate::store::Store;

    async fn running_task() -> AppState {
        let config = Config {
            home: std::env::temp_dir().join(format!("delilah-hooks-{}", uuid::Uuid::new_v4())),
            addr: "127.0.0.1:7420".parse().unwrap(),
            allow_file_protocol: false,
            notify: false,
            ui_origin: None,
            claude_bin: "claude".into(),
            agent_program: None,
            ..crate::config::Config::test_defaults()
        };
        let state = AppState::new(Store::in_memory().await.unwrap(), config);
        let key = TaskKey::new("PROJ-1");
        state
            .store
            .create_task(&Task::new(key.clone(), "t", "feature"))
            .await
            .unwrap();
        state
            .store
            .apply_event(&key, Event::Start, Utc::now())
            .await
            .unwrap();
        state
            .store
            .apply_event(&key, Event::WorkspaceReady, Utc::now())
            .await
            .unwrap();
        state
    }

    async fn post(state: &AppState, path: &str, body: Value) -> StatusCode {
        let req = Request::post(path)
            .header("content-type", "application/json")
            .body(Body::from(body.to_string()))
            .unwrap();
        crate::api::router(state.clone())
            .oneshot(req)
            .await
            .unwrap()
            .status()
    }

    fn scratch() -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("delilah-tp-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_hook_transcript_must_resolve_inside_the_projects_root() {
        let base = scratch();
        let root = base.join("projects");
        std::fs::create_dir_all(root.join("p")).unwrap();
        let inside = root.join("p/s.jsonl");
        std::fs::write(&inside, "{}\n").unwrap();
        let outside = base.join("secret.jsonl");
        std::fs::write(&outside, "{}\n").unwrap();
        std::os::unix::fs::symlink(&outside, root.join("p/link.jsonl")).unwrap();
        let hook = |p: &std::path::Path| json!({ "transcript_path": p });

        assert!(trusted_transcript_in(&root, &hook(&inside)).is_some());
        assert!(trusted_transcript_in(&root, &hook(&outside)).is_none());
        assert!(trusted_transcript_in(&root, &hook(&root.join("p/link.jsonl"))).is_none());
        assert!(trusted_transcript_in(&root, &hook(&root.join("p/none.jsonl"))).is_none());
        assert!(trusted_transcript_in(&root, &hook(&root.join("p/../../secret.jsonl"))).is_none());
        assert!(trusted_transcript_in(&root, &hook(std::path::Path::new("rel.jsonl"))).is_none());
    }

    #[tokio::test]
    async fn a_forged_task_cannot_write_outside_the_task_dirs() {
        let state = running_task().await;
        let outside = state.config.home.join("outside");
        for task in ["..%2Foutside", "%2Ftmp%2Fdelilah-x", ".."] {
            let path = format!("/hooks/claude?task={task}");
            let code = post(&state, &path, json!({"hook_event_name": "Stop"})).await;
            assert_eq!(code, StatusCode::BAD_REQUEST, "{task}");
        }
        assert!(!outside.exists());
        let code = post(
            &state,
            "/hooks/claude?task=%40svc",
            json!({"hook_event_name": "Stop"}),
        )
        .await;
        assert_eq!(code, StatusCode::NO_CONTENT);
    }

    async fn status(state: &AppState) -> TaskStatus {
        state
            .store
            .get_task(&TaskKey::new("PROJ-1"))
            .await
            .unwrap()
            .unwrap()
            .status
    }

    async fn inbox(state: &AppState) -> Vec<Value> {
        let req = Request::get("/api/v1/inbox").body(Body::empty()).unwrap();
        let res = crate::api::router(state.clone())
            .oneshot(req)
            .await
            .unwrap();
        let body = axum::body::to_bytes(res.into_body(), usize::MAX)
            .await
            .unwrap();
        serde_json::from_slice(&body).unwrap()
    }

    #[tokio::test]
    async fn what_the_agent_asks_reaches_the_inbox_until_answered() {
        let state = running_task().await;
        state
            .store
            .create_session(&crate::store_sessions::SessionRecord {
                id: "s-1".into(),
                task: "PROJ-1".into(),
                kind: crate::store_sessions::SessionKind::Claude,
                label: "Principal".into(),
                primary: true,
                closed: false,
                read_only: false,
                conversation: false,
                step: None,
            })
            .await
            .unwrap();
        let url = "/hooks/claude?task=PROJ-1&session=s-1";
        let stop = json!({ "hook_event_name": "Stop", "last_assistant_message": "¿Pagino con cursor u offset?" });
        post(&state, url, stop).await;
        // The idle reminder that follows does not hide the question.
        let idle = json!({ "hook_event_name": "Notification", "notification_type": "idle_prompt", "message": "Claude is waiting for your input" });
        post(&state, url, idle).await;

        let items = inbox(&state).await;
        assert_eq!(items.len(), 1, "{items:?}");
        assert_eq!(items[0]["kind"], "question");
        assert_eq!(items[0]["text"], "¿Pagino con cursor u offset?");
        assert_eq!(items[0]["session"]["id"], "s-1");

        post(
            &state,
            url,
            json!({ "hook_event_name": "UserPromptSubmit" }),
        )
        .await;
        assert!(inbox(&state).await.is_empty());
    }

    /// A live Claude session `s-1` of PROJ-1 (a quiet process behind it).
    fn live_claude(state: &AppState) {
        let mut cmd = portable_pty::CommandBuilder::new("bash");
        cmd.args(["--noprofile", "--norc", "-c", "sleep 5"]);
        let pty = delilah_agents::pty::PtySession::spawn(cmd, 80, 24, None).unwrap();
        state.sessions.insert(
            "s-1",
            "PROJ-1",
            crate::store_sessions::SessionKind::Claude,
            true,
            pty,
        );
    }

    async fn activity(state: &AppState) -> Value {
        let req = Request::get("/api/v1/activity").body(Body::empty()).unwrap();
        let res = crate::api::router(state.clone())
            .oneshot(req)
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = axum::body::to_bytes(res.into_body(), usize::MAX)
            .await
            .unwrap();
        serde_json::from_slice(&body).unwrap()
    }

    const LIVE: &str = "/hooks/claude?task=PROJ-1&session=s-1";

    fn pre_edit() -> Value {
        json!({ "hook_event_name": "PreToolUse", "tool_name": "Edit", "tool_use_id": "t1",
                "tool_input": { "file_path": "/repo/src/main.rs" } })
    }

    #[tokio::test]
    async fn pre_tool_use_is_not_logged_but_post_tool_use_is() {
        let state = running_task().await;
        live_claude(&state);
        let log = state.config.task_dir("PROJ-1").join("hooks.jsonl");
        let lines = || std::fs::read_to_string(&log).map_or(0, |s| s.lines().count());
        post(&state, LIVE, pre_edit()).await;
        assert_eq!(lines(), 0);
        post(
            &state,
            LIVE,
            json!({ "hook_event_name": "PostToolUse", "tool_name": "Edit", "tool_use_id": "t1" }),
        )
        .await;
        assert_eq!(lines(), 1);
    }

    #[tokio::test]
    async fn a_tool_call_shows_in_the_activity_until_it_ends() {
        let state = running_task().await;
        live_claude(&state);
        post(&state, LIVE, pre_edit()).await;
        let a = activity(&state).await;
        assert_eq!(a["sessions"]["s-1"]["action"]["kind"], "edit");
        assert_eq!(a["sessions"]["s-1"]["action"]["target"], "main.rs");

        post(
            &state,
            LIVE,
            json!({ "hook_event_name": "PostToolUse", "tool_name": "Edit", "tool_use_id": "t1" }),
        )
        .await;
        assert_eq!(activity(&state).await, json!({ "sessions": {} }));
    }

    #[tokio::test]
    async fn waiting_clears_the_activity() {
        let state = running_task().await;
        live_claude(&state);
        post(&state, LIVE, pre_edit()).await;
        assert!(activity(&state).await["sessions"]["s-1"].is_object());
        post(&state, LIVE, json!({ "hook_event_name": "Notification" })).await;
        assert_eq!(activity(&state).await, json!({ "sessions": {} }));
        assert_eq!(
            state.sessions.get("s-1").unwrap().state(),
            LiveState::Waiting
        );
    }

    #[tokio::test]
    async fn a_tool_call_of_an_unknown_session_is_ignored() {
        let state = running_task().await;
        let code = post(
            &state,
            "/hooks/claude?task=PROJ-1&session=nope",
            pre_edit(),
        )
        .await;
        assert_eq!(code, StatusCode::NO_CONTENT);
        assert_eq!(activity(&state).await, json!({ "sessions": {} }));
    }

    #[tokio::test]
    async fn activity_is_announced_on_change_only() {
        let state = running_task().await;
        live_claude(&state);
        let mut rx = state.events.subscribe();
        let drain = |rx: &mut tokio::sync::broadcast::Receiver<Topic>| {
            let mut topics = vec![];
            while let Ok(t) = rx.try_recv() {
                topics.push(t);
            }
            topics
        };
        post(&state, LIVE, pre_edit()).await;
        assert!(drain(&mut rx).contains(&Topic::Activity));
        // The end of a call that is not the current one changes nothing.
        let other = json!({ "hook_event_name": "PostToolUse", "tool_use_id": "other" });
        post(&state, LIVE, other).await;
        assert!(!drain(&mut rx).contains(&Topic::Activity));
    }

    fn session_record(id: &str, primary: bool) -> crate::store_sessions::SessionRecord {
        crate::store_sessions::SessionRecord {
            id: id.into(),
            task: "PROJ-1".into(),
            kind: crate::store_sessions::SessionKind::Claude,
            label: id.into(),
            primary,
            closed: false,
            read_only: false,
            conversation: false,
            step: None,
        }
    }

    #[tokio::test]
    async fn stop_and_prompt_hooks_set_the_sessions_last_activity() {
        let state = running_task().await;
        state
            .store
            .create_session(&session_record("s-1", true))
            .await
            .unwrap();
        let url = "/hooks/claude?task=PROJ-1&session=s-1";
        post(
            &state,
            url,
            json!({ "hook_event_name": "UserPromptSubmit", "prompt": "fix the bug" }),
        )
        .await;
        let extras = state.store.session_extras("s-1").await.unwrap();
        assert_eq!(extras.last_message.as_deref(), Some("fix the bug"));
        assert!(extras.last_activity_at.is_some());

        let long = "y".repeat(500);
        post(
            &state,
            url,
            json!({ "hook_event_name": "Stop", "last_assistant_message": long }),
        )
        .await;
        let extras = state.store.session_extras("s-1").await.unwrap();
        assert_eq!(extras.last_message.unwrap().chars().count(), 200);
    }

    #[tokio::test]
    async fn a_stop_of_a_non_primary_session_is_a_turn_done_aviso_and_a_primary_one_is_not() {
        let state = running_task().await;
        state
            .store
            .create_session(&session_record("s-1", true))
            .await
            .unwrap();
        state
            .store
            .create_session(&session_record("s-2", false))
            .await
            .unwrap();
        let stop = json!({ "hook_event_name": "Stop", "last_assistant_message": "listo" });
        post(
            &state,
            "/hooks/claude?task=PROJ-1&session=s-1",
            stop.clone(),
        )
        .await;
        let items = inbox(&state).await;
        assert!(items.iter().all(|i| i["kind"] != "turn-done"), "{items:?}");
        assert!(items.iter().any(|i| i["kind"] == "question"));

        post(&state, "/hooks/claude?task=PROJ-1&session=s-2", stop).await;
        let items = inbox(&state).await;
        let done: Vec<_> = items.iter().filter(|i| i["kind"] == "turn-done").collect();
        assert_eq!(done.len(), 1, "{items:?}");
        assert_eq!(done[0]["session"]["id"], "s-2");
        assert_eq!(done[0]["text"], "listo");
        assert!(done[0]["notice"].is_i64());
    }

    #[tokio::test]
    async fn turns_move_the_task_between_running_and_waiting() {
        let state = running_task().await;
        let hook = |name: &str| json!({ "hook_event_name": name, "session_id": "abc" });

        // The first prompt arrives while already running: ignored, not an error.
        assert_eq!(
            post(
                &state,
                "/hooks/claude?task=PROJ-1",
                hook("UserPromptSubmit")
            )
            .await,
            StatusCode::NO_CONTENT
        );
        assert_eq!(status(&state).await, TaskStatus::Running);

        post(&state, "/hooks/claude?task=PROJ-1", hook("Stop")).await;
        assert_eq!(status(&state).await, TaskStatus::WaitingForUser);
        post(&state, "/hooks/claude?task=PROJ-1", hook("Notification")).await;
        assert_eq!(status(&state).await, TaskStatus::WaitingForUser);

        post(
            &state,
            "/hooks/claude?task=PROJ-1",
            hook("UserPromptSubmit"),
        )
        .await;
        assert_eq!(status(&state).await, TaskStatus::Running);

        let raw =
            std::fs::read_to_string(state.config.task_dir("PROJ-1").join("hooks.jsonl")).unwrap();
        assert_eq!(raw.lines().count(), 4);
    }

    #[tokio::test]
    async fn keeps_the_latest_usage_windows() {
        let state = running_task().await;
        let payload = json!({ "rate_limits": { "five_hour": { "used_percentage": 82, "resets_at": 1792422000 } } });

        post(&state, "/hooks/claude/statusline?task=PROJ-1", payload).await;
        let usage = state.latest_limits().unwrap();
        assert_eq!(usage["five_hour"]["used_percentage"], 82);
    }

    #[tokio::test]
    async fn a_hook_sets_and_replaces_the_live_transcript_path() {
        let state = running_task().await;
        let mut cmd = portable_pty::CommandBuilder::new("sleep");
        cmd.arg("30");
        let pty = delilah_agents::pty::PtySession::spawn(cmd, 80, 24, None).unwrap();
        let live = state.sessions.insert(
            "s-1",
            "PROJ-1",
            crate::store_sessions::SessionKind::Claude,
            true,
            pty,
        );
        assert_eq!(live.transcript(), None);
        let url = "/hooks/claude?task=PROJ-1&session=s-1";
        let hook = |path: &std::path::Path| json!({ "hook_event_name": "PostToolUse", "transcript_path": path });
        let root = state.config.claude_projects.clone();
        std::fs::create_dir_all(&root).unwrap();
        let file = |name: &str| {
            let path = root.join(format!("{}-{name}.jsonl", uuid::Uuid::new_v4()));
            std::fs::write(&path, "{}\n").unwrap();
            path.canonicalize().unwrap()
        };
        let (a, b) = (file("a"), file("b"));
        post(&state, url, hook(&a)).await;
        assert_eq!(live.transcript(), Some(a));
        post(&state, url, hook(&b)).await;
        assert_eq!(live.transcript(), Some(b.clone()));
        // Not absolute, not a transcript, climbing out, outside the projects folder or a
        // link to a file outside it: ignored, and the current path stays.
        let outside = scratch().join("x.jsonl");
        std::fs::write(&outside, "{}\n").unwrap();
        let link = root.join(format!("{}-link.jsonl", uuid::Uuid::new_v4()));
        std::os::unix::fs::symlink(&outside, &link).unwrap();
        for bad in [
            std::path::Path::new("rel/c.jsonl"),
            std::path::Path::new("/etc/passwd"),
            &root.join("../etc/c.jsonl"),
            &outside,
            &link,
        ] {
            post(&state, url, hook(bad)).await;
            assert_eq!(live.transcript(), Some(b.clone()), "{bad:?}");
        }
    }
}

#[cfg(test)]
mod limit_tests {
    use axum::body::Body;
    use axum::http::Request;
    use delilah_core::{Task, TaskStatus};
    use serde_json::json;
    use tower::ServiceExt;

    use super::*;
    use crate::config::Config;
    use crate::store::Store;

    async fn running() -> AppState {
        let config = Config {
            home: std::env::temp_dir().join(format!("delilah-limits-{}", uuid::Uuid::new_v4())),
            addr: "127.0.0.1:7420".parse().unwrap(),
            allow_file_protocol: false,
            notify: false,
            ui_origin: None,
            claude_bin: "claude".into(),
            agent_program: None,
            ..crate::config::Config::test_defaults()
        };
        let state = AppState::new(Store::in_memory().await.unwrap(), config);
        let key = TaskKey::new("PROJ-1");
        state
            .store
            .create_task(&Task::new(key.clone(), "Card retries", "feature"))
            .await
            .unwrap();
        state.transition(&key, Event::Start).await.unwrap();
        state.transition(&key, Event::WorkspaceReady).await.unwrap();
        state
    }

    async fn post(state: &AppState, path: &str, body: Value) {
        let req = Request::post(path)
            .header("content-type", "application/json")
            .body(Body::from(body.to_string()))
            .unwrap();
        crate::api::router(state.clone())
            .oneshot(req)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn a_turn_ending_with_a_full_window_pauses_until_its_reset() {
        let state = running().await;
        let reset = (Utc::now() + chrono::Duration::hours(1)).timestamp();
        post(&state, "/hooks/claude/statusline?task=PROJ-1", json!({ "rate_limits": { "five_hour": { "used_percentage": 100, "resets_at": reset } } })).await;
        post(
            &state,
            "/hooks/claude?task=PROJ-1",
            json!({ "hook_event_name": "StopFailure" }),
        )
        .await;

        let task = state
            .store
            .get_task(&TaskKey::new("PROJ-1"))
            .await
            .unwrap()
            .unwrap();
        let TaskStatus::PausedByLimit { resume_at } = task.status else {
            panic!("{:?}", task.status)
        };
        assert_eq!(resume_at.timestamp(), reset);
        let sent = state.notifier.recorded();
        assert_eq!(sent.last().unwrap().title, "PROJ-1 pausada por límite");
    }

    #[tokio::test]
    async fn a_normal_turn_end_waits_for_the_user_and_notifies() {
        let state = running().await;
        post(
            &state,
            "/hooks/claude?task=PROJ-1",
            json!({ "hook_event_name": "Stop" }),
        )
        .await;

        let task = state
            .store
            .get_task(&TaskKey::new("PROJ-1"))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(task.status, TaskStatus::WaitingForUser);
        assert_eq!(
            state.notifier.recorded().last().unwrap().title,
            "PROJ-1 te espera"
        );

        // A tool running again (e.g. after approving a permission) means it is working.
        post(
            &state,
            "/hooks/claude?task=PROJ-1",
            json!({ "hook_event_name": "PostToolUse" }),
        )
        .await;
        let task = state
            .store
            .get_task(&TaskKey::new("PROJ-1"))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(task.status, TaskStatus::Running);
    }
}
