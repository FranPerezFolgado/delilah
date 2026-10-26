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
use std::sync::atomic::Ordering;
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
pub(crate) async fn apply_signal(
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
    let terminal = live.as_ref().is_some_and(|l| l.runtime.as_pty().is_some());
    let task_session = crate::store_sessions::project_of(&q.task).is_none();
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
        // Only a new turn makes a held Stop stale. A tool hook (`Working`) does not: a
        // background sub-agent posts them to this same session after the turn's Stop.
        let agent_start = signal == Signal::PromptSubmitted && agent_prompt(&hook);
        // A terminal has no stream to tell a turn's start: with its Stop held, the turn is
        // over, so an agent prompt now begins the next one.
        let agent_turn = agent_start
            && (live.state() == LiveState::Waiting || (terminal && live.has_held_stop()));
        if signal == Signal::PromptSubmitted && (!agent_start || agent_turn) {
            live.drop_held_stop();
        }
        // The last sub-agent a terminal's held Stop waits on ended: if Claude does not
        // start a turn about it, the Stop applies after the fallback.
        if terminal
            && hook["hook_event_name"] == "SubagentStop"
            && let Some(id) = hook["agent_id"].as_str()
            && live.held_agent_ended(id)
        {
            crate::stream_events::arm_fallback(&app, live.clone(), &q.task);
        }
        // Claude Code's idle reminder, a minute after a turn ended, is no question while
        // that end is held behind sub-agents still working.
        if signal == Signal::NeedsUser
            && hook["notification_type"] == "idle_prompt"
            && live.has_held_stop()
        {
            return;
        }
        // A background sub-agent's tool hook (it carries `agent_id`) arriving once the
        // session waits again is not a turn: it must not flip the session back to working.
        if signal == Signal::Working
            && hook.get("agent_id").is_some()
            && live.state() == LiveState::Waiting
        {
            return;
        }
        // The CLI also posts the `<task-notification>` prompt mid-turn, when a background
        // task ends during it: no new turn, so nothing of a prompt's effects applies (it
        // would clear a pending question the turn still waits on). Waiting, it is the
        // start of a turn Claude began itself and applies in full.
        if agent_start && !agent_turn {
            return;
        }
        // Background work still runs: the turn's end is held, not applied (see
        // `Live::hold_stop_if_background`, the one atomic check; a terminal goes by the
        // Stop's own `background_tasks`, `Live::hold_stop_if_reported`). Queued messages are
        // typed now so they are not stranded until the replay.
        if signal == Signal::TurnEnded
            && limit_reset.is_none()
            && (if terminal {
                live.hold_stop_if_reported(hook.clone())
            } else {
                live.runtime.as_stream().is_some()
                    && !next_turn_started(live)
                    && live.hold_stop_if_background(hook.clone())
            })
        {
            crate::conversation::flush_queue(&app, live).await;
            // A terminal's turn end still does its phase work (the handoff, the stall
            // check) at once, as it always did: only the waiting is held. Its replay
            // does not do it again.
            if terminal && task_session {
                crate::phases::on_hook(&app, &q.task, signal, &hook, primary, false).await;
            }
            return;
        }
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
        // Snapshot before locking the log: never call `sessions` under its lock.
        let live_ids: std::collections::HashSet<String> =
            app.sessions.all().into_iter().map(|l| l.id).collect();
        app.activity_log
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .record(&live.id, &live.task, Utc::now(), event.clone(), &live_ids);
        // A change is announced at once; an append that changes nothing waits for the tick.
        if !live.update_activity(event) {
            app.activity_dirty.store(true, Ordering::Relaxed);
        }
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
    let replayed_in_terminal = terminal && hook["delilah_replayed"] == true;
    if task_session && !replayed_in_terminal {
        crate::phases::on_hook(&app, &q.task, signal, &hook, primary, limit_reset.is_some()).await;
    }

    // A project's session has no task to move: only the session and the inbox change.
    let event = event.filter(|_| task_session);
    if let Some(event) = event {
        match app.transition(&TaskKey::new(&q.task), event).await {
            // Hooks repeat and overlap (a Notification after Stop, the first prompt
            // while already running): a transition that does not apply is not an error.
            Ok(_) | Err(StoreError::Transition(_)) => {}
            Err(e) => tracing::warn!(task = %q.task, "hook not applied: {e}"),
        }
    }
}

/// Whether the turn after the one this `Stop` ends already began (its `Init` came first):
/// the hook is late and belongs to the past, so it is applied, never held.
fn next_turn_started(live: &crate::sessions::Live) -> bool {
    live.progress()
        .turn
        .is_some_and(|t| t.started_at > Utc::now() - chrono::Duration::seconds(1))
}

/// Whether a prompt hook is not the user's: the one the daemon makes up for a turn Claude
/// started by itself (`delilah_origin`), or the `<task-notification>` XML the CLI posts
/// as a prompt when a background task ends.
pub(crate) fn agent_prompt(hook: &Value) -> bool {
    hook["delilah_origin"].as_str() == Some("agent")
        || hook["prompt"]
            .as_str()
            .is_some_and(|p| p.trim_start().starts_with("<task-notification>"))
}

/// A finished turn or a submitted prompt is the session's last activity; Claude's closing
/// message (or the prompt) is what its row shows.
async fn note_activity(app: &AppState, session: &str, signal: Signal, hook: &Value) {
    let message = match signal {
        Signal::TurnEnded => hook["last_assistant_message"].as_str(),
        // A turn Claude started itself has no prompt of the user's to show.
        Signal::PromptSubmitted if agent_prompt(hook) => return,
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
        state.lang.set(crate::lang::Lang::Es); // Spanish fixtures
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
        let req = Request::get("/api/v1/activity")
            .body(Body::empty())
            .unwrap();
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

    async fn history(state: &AppState, query: &str) -> (StatusCode, Value) {
        let req = Request::get(format!("/api/v1/activity/history{query}"))
            .body(Body::empty())
            .unwrap();
        let res = crate::api::router(state.clone())
            .oneshot(req)
            .await
            .unwrap();
        let code = res.status();
        let body = axum::body::to_bytes(res.into_body(), usize::MAX)
            .await
            .unwrap();
        (code, serde_json::from_slice(&body).unwrap_or(Value::Null))
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
        assert_eq!(
            activity(&state).await,
            json!({ "sessions": {}, "tokens": {} })
        );
    }

    #[tokio::test]
    async fn waiting_clears_the_activity() {
        let state = running_task().await;
        live_claude(&state);
        post(&state, LIVE, pre_edit()).await;
        assert!(activity(&state).await["sessions"]["s-1"].is_object());
        post(&state, LIVE, json!({ "hook_event_name": "Notification" })).await;
        assert_eq!(
            activity(&state).await,
            json!({ "sessions": {}, "tokens": {} })
        );
        assert_eq!(
            state.sessions.get("s-1").unwrap().state(),
            LiveState::Waiting
        );
    }

    #[tokio::test]
    async fn a_tool_call_of_an_unknown_session_is_ignored() {
        let state = running_task().await;
        let code = post(&state, "/hooks/claude?task=PROJ-1&session=nope", pre_edit()).await;
        assert_eq!(code, StatusCode::NO_CONTENT);
        assert_eq!(
            activity(&state).await,
            json!({ "sessions": {}, "tokens": {} })
        );
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

    #[tokio::test]
    async fn a_no_change_append_is_recorded_and_marks_the_log_dirty() {
        let state = running_task().await;
        live_claude(&state);
        post(&state, LIVE, pre_edit()).await;
        state.activity_dirty.store(false, Ordering::Relaxed);
        // The end of a call that is not the current one changes nothing, but is logged.
        let other = json!({ "hook_event_name": "PostToolUse", "tool_use_id": "other" });
        post(&state, LIVE, other).await;
        assert!(state.activity_dirty.load(Ordering::Relaxed));
        let (code, h) = history(&state, "?owner=PROJ-1").await;
        assert_eq!(code, StatusCode::OK);
        assert_eq!(h["sessions"][0]["entries"].as_array().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn history_returns_entries_after_a_seq_and_keeps_ended_sessions() {
        let state = running_task().await;
        live_claude(&state);
        post(&state, LIVE, pre_edit()).await;
        let (_, h) = history(&state, "?owner=PROJ-1").await;
        assert!(h["started_at"].is_string());
        assert_eq!(h["sessions"][0]["session"], "s-1");
        assert_eq!(h["sessions"][0]["live"], true);
        assert_eq!(h["sessions"][0]["entries"][0]["type"], "tool_started");
        let seq = h["sessions"][0]["entries"][0]["seq"].as_u64().unwrap();

        let end = json!({ "hook_event_name": "PostToolUse", "tool_use_id": "t1" });
        post(&state, LIVE, end).await;
        let (_, h) = history(&state, &format!("?owner=PROJ-1&after={seq}")).await;
        let entries = h["sessions"][0]["entries"].as_array().unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0]["type"], "tool_ended");

        let last = entries[0]["seq"].as_u64().unwrap();
        let live = state.sessions.get("s-1").unwrap();
        state.sessions.remove_if_same("s-1", &live.runtime);
        let (_, h) = history(&state, &format!("?owner=PROJ-1&after={last}")).await;
        assert_eq!(h["sessions"][0]["live"], false);
        assert!(h["sessions"][0]["entries"].as_array().unwrap().is_empty());

        let (code, h) = history(&state, "?owner=NOPE-1").await;
        assert_eq!(
            (code, h["sessions"].as_array().map(Vec::len)),
            (StatusCode::OK, Some(0))
        );
    }

    #[tokio::test]
    async fn history_rejects_a_missing_owner_and_a_bad_after() {
        let state = running_task().await;
        assert_eq!(history(&state, "").await.0, StatusCode::BAD_REQUEST);
        assert_eq!(history(&state, "?owner=").await.0, StatusCode::BAD_REQUEST);
        assert_eq!(
            history(&state, "?owner=PROJ-1&after=x").await.0,
            StatusCode::BAD_REQUEST
        );
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
    async fn a_prompt_the_agent_made_up_or_a_task_notification_is_not_the_users_last_message() {
        let state = running_task().await;
        state
            .store
            .create_session(&session_record("s-1", true))
            .await
            .unwrap();
        let url = "/hooks/claude?task=PROJ-1&session=s-1";
        for hook in [
            json!({ "hook_event_name": "UserPromptSubmit", "delilah_origin": "agent" }),
            json!({ "hook_event_name": "UserPromptSubmit",
                    "prompt": "<task-notification>\n<task-id>b1</task-id>" }),
        ] {
            post(&state, url, hook).await;
            let extras = state.store.session_extras("s-1").await.unwrap();
            assert_eq!(extras.last_message, None);
            assert_eq!(extras.last_activity_at, None);
        }
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

    // --- Held Stop (background work running) ---

    const URL: &str = "/hooks/claude?task=PROJ-1&session=s-1";

    /// A chat-mode session `id` of PROJ-1 (its process just sleeps), working.
    async fn stream_session(state: &AppState, id: &str, primary: bool) -> crate::sessions::Live {
        state
            .store
            .create_session(&session_record(id, primary))
            .await
            .unwrap();
        let mut cmd = tokio::process::Command::new("sleep");
        cmd.arg("30");
        let (session, _rx) = delilah_agents::stream::StreamSession::spawn(cmd).unwrap();
        let live = state.sessions.insert(
            id,
            "PROJ-1",
            crate::store_sessions::SessionKind::Claude,
            primary,
            std::sync::Arc::new(session),
        );
        live.set_state(LiveState::Working);
        live
    }

    fn feed(live: &crate::sessions::Live, event: delilah_agents::stream::StreamEvent) {
        live.update_progress(|p| crate::progress::apply(p, &event, Utc::now()));
    }

    fn bg_started() -> delilah_agents::stream::StreamEvent {
        use delilah_agents::stream::{StreamEvent, TaskEvent, TaskKind};
        StreamEvent::Task(TaskEvent::Started {
            task_id: "b1".into(),
            tool_use_id: None,
            kind: TaskKind::Command,
            description: "sleep".into(),
            background: true,
        })
    }

    fn init() -> delilah_agents::stream::StreamEvent {
        delilah_agents::stream::StreamEvent::Init {
            session_id: "x".into(),
            self_started: false,
        }
    }

    fn stop_hook() -> Value {
        json!({ "hook_event_name": "Stop", "last_assistant_message": "listo" })
    }

    #[tokio::test]
    async fn a_stop_with_background_running_is_held_with_no_end_of_turn_effects() {
        let state = running_task().await;
        let live = stream_session(&state, "s-1", true).await;
        let other = stream_session(&state, "s-2", false).await;
        feed(&live, bg_started());
        feed(&other, bg_started());

        post(&state, URL, stop_hook()).await;
        post(&state, "/hooks/claude?task=PROJ-1&session=s-2", stop_hook()).await;

        assert_eq!(live.state(), LiveState::Working);
        assert_eq!(other.state(), LiveState::Working);
        assert!(live.has_held_stop());
        assert!(other.has_held_stop());
        assert_eq!(status(&state).await, TaskStatus::Running);
        // No question in the inbox, no "turn done" aviso, no last message.
        assert!(inbox(&state).await.is_empty());
        let extras = state.store.session_extras("s-1").await.unwrap();
        assert_eq!(extras.last_message, None);
    }

    #[tokio::test]
    async fn a_stop_with_no_background_applies_as_before() {
        let state = running_task().await;
        let live = stream_session(&state, "s-1", true).await;
        post(&state, URL, stop_hook()).await;
        assert_eq!(live.state(), LiveState::Waiting);
        assert!(!live.has_held_stop());
        assert_eq!(status(&state).await, TaskStatus::WaitingForUser);
        assert_eq!(inbox(&state).await.len(), 1);
    }

    #[tokio::test]
    async fn the_hold_stores_nothing_when_the_background_set_is_empty() {
        let state = running_task().await;
        let live = stream_session(&state, "s-1", true).await;
        assert!(!live.hold_stop_if_background(stop_hook()));
        assert!(!live.has_held_stop());
        feed(&live, bg_started());
        assert!(live.hold_stop_if_background(stop_hook()));
        // The replay: nothing runs any more, so it applies, whole.
        feed(
            &live,
            delilah_agents::stream::StreamEvent::Task(delilah_agents::stream::TaskEvent::Ended {
                task_id: "b1".into(),
                status: delilah_agents::stream::TaskStatus::Completed,
                summary: None,
            }),
        );
        let held = live.take_held_stop().unwrap();
        apply_signal(
            state.clone(),
            "PROJ-1",
            Some("s-1"),
            Signal::TurnEnded,
            &held,
        )
        .await;
        assert_eq!(live.state(), LiveState::Waiting);
        assert!(!live.has_held_stop());
        assert_eq!(status(&state).await, TaskStatus::WaitingForUser);
    }

    #[tokio::test]
    async fn a_new_turn_drops_the_held_stop_but_an_agent_prompt_mid_turn_does_not() {
        let state = running_task().await;
        let live = stream_session(&state, "s-1", true).await;
        feed(&live, bg_started());

        for hook in [json!({ "hook_event_name": "UserPromptSubmit", "prompt": "hola" })] {
            post(&state, URL, stop_hook()).await;
            assert!(live.has_held_stop());
            post(&state, URL, hook).await;
            assert!(!live.has_held_stop());
            assert_eq!(live.state(), LiveState::Working);
        }

        post(&state, URL, stop_hook()).await;
        post(
            &state,
            URL,
            json!({ "hook_event_name": "UserPromptSubmit",
                    "prompt": "<task-notification>\n<task-id>b1</task-id>" }),
        )
        .await;
        assert!(live.has_held_stop(), "the agent prompt is not a turn start");
    }

    #[tokio::test]
    async fn a_background_sub_agents_tool_hook_neither_drops_the_held_stop_nor_wakes_a_waiting_session()
     {
        let state = running_task().await;
        let live = stream_session(&state, "s-1", true).await;
        feed(&live, bg_started());
        post(&state, URL, stop_hook()).await;
        assert!(live.has_held_stop());

        let sub = json!({ "hook_event_name": "PostToolUse", "tool_name": "Bash",
                          "agent_id": "a696ef4688000ea5e" });
        post(&state, URL, sub.clone()).await;
        assert!(live.has_held_stop(), "a sub-agent tool is not a new turn");
        assert_eq!(live.state(), LiveState::Working);

        // The set empties: the replay applies the Stop.
        feed(
            &live,
            delilah_agents::stream::StreamEvent::Task(delilah_agents::stream::TaskEvent::Ended {
                task_id: "b1".into(),
                status: delilah_agents::stream::TaskStatus::Completed,
                summary: None,
            }),
        );
        let held = live.take_held_stop().unwrap();
        apply_signal(
            state.clone(),
            "PROJ-1",
            Some("s-1"),
            Signal::TurnEnded,
            &held,
        )
        .await;
        assert_eq!(live.state(), LiveState::Waiting);

        // A late sub-agent hook does not flip it back.
        post(&state, URL, sub).await;
        assert_eq!(live.state(), LiveState::Waiting);
        assert_eq!(status(&state).await, TaskStatus::WaitingForUser);
    }

    #[tokio::test]
    async fn a_stop_after_the_next_turns_init_is_applied_never_held() {
        let state = running_task().await;
        let live = stream_session(&state, "s-1", true).await;
        feed(&live, bg_started());
        feed(&live, init());
        post(&state, URL, stop_hook()).await;
        assert!(!live.has_held_stop());
        assert_eq!(live.state(), LiveState::Waiting);
    }

    #[tokio::test]
    async fn a_failed_turn_a_usage_limit_and_a_terminal_session_are_never_held() {
        let state = running_task().await;
        let live = stream_session(&state, "s-1", true).await;
        feed(&live, bg_started());

        post(
            &state,
            URL,
            json!({ "hook_event_name": "StopFailure", "error": "server_error" }),
        )
        .await;
        assert!(!live.has_held_stop());
        assert_eq!(live.state(), LiveState::Waiting);

        // A terminal session has no progress to wait on.
        let mut cmd = portable_pty::CommandBuilder::new("sleep");
        cmd.arg("30");
        let pty = delilah_agents::pty::PtySession::spawn(cmd, 80, 24, None).unwrap();
        state
            .store
            .create_session(&session_record("s-3", false))
            .await
            .unwrap();
        let term = state.sessions.insert(
            "s-3",
            "PROJ-1",
            crate::store_sessions::SessionKind::Claude,
            false,
            pty,
        );
        term.set_state(LiveState::Working);
        feed(&term, bg_started());
        post(&state, "/hooks/claude?task=PROJ-1&session=s-3", stop_hook()).await;
        assert!(!term.has_held_stop());
        assert_eq!(term.state(), LiveState::Waiting);

        // Last: the full window stays in the usage report.
        live.set_state(LiveState::Working);
        let reset = (Utc::now() + chrono::Duration::hours(1)).timestamp();
        post(&state, "/hooks/claude/statusline?task=PROJ-1", json!({ "rate_limits": { "five_hour": { "used_percentage": 100, "resets_at": reset } } })).await;
        post(&state, URL, stop_hook()).await;
        assert!(!live.has_held_stop());
        assert_eq!(live.state(), LiveState::Paused);
    }

    #[tokio::test]
    async fn an_agent_prompt_mid_turn_keeps_a_pending_question_and_the_turn_state() {
        let state = running_task().await;
        let live = stream_session(&state, "s-1", true).await;
        // A question is pending (a permission card, say) while the turn is still going.
        state
            .store
            .set_attention(
                "s-1",
                Some(&crate::inbox::Attention {
                    kind: crate::inbox::AttentionKind::Permission,
                    text: "Bash".into(),
                    since: Utc::now(),
                }),
            )
            .await
            .unwrap();
        live.update_convo(|c| c.needs_terminal = true);

        post(
            &state,
            URL,
            json!({ "hook_event_name": "UserPromptSubmit",
                    "prompt": "<task-notification>\n<task-id>b1</task-id>" }),
        )
        .await;
        assert_eq!(live.state(), LiveState::Working);
        assert!(state.store.attention("s-1").await.unwrap().is_some());
        assert!(live.needs_terminal());
        assert_eq!(status(&state).await, TaskStatus::Running);

        // Waiting (the self-started turn): it does what it did.
        live.set_state(LiveState::Waiting);
        post(
            &state,
            URL,
            json!({ "hook_event_name": "UserPromptSubmit", "delilah_origin": "agent" }),
        )
        .await;
        assert_eq!(live.state(), LiveState::Working);
        assert!(state.store.attention("s-1").await.unwrap().is_none());
    }

    /// A terminal session `id` of PROJ-1 (its process just sleeps), working.
    async fn pty_session(state: &AppState, id: &str) -> crate::sessions::Live {
        state
            .store
            .create_session(&session_record(id, true))
            .await
            .unwrap();
        let mut cmd = portable_pty::CommandBuilder::new("sleep");
        cmd.arg("30");
        let pty = delilah_agents::pty::PtySession::spawn(cmd, 80, 24, None).unwrap();
        let live = state.sessions.insert(
            id,
            "PROJ-1",
            crate::store_sessions::SessionKind::Claude,
            true,
            pty,
        );
        live.set_state(LiveState::Working);
        live
    }

    /// A Stop as a terminal's Claude Code sends it, `agents` running in the background.
    fn terminal_stop(agents: &[&str]) -> Value {
        let tasks: Vec<Value> = agents
            .iter()
            .map(|id| {
                json!({ "id": id, "type": "subagent", "status": "running",
                              "agent_type": "implementer", "description": "d" })
            })
            .collect();
        json!({ "hook_event_name": "Stop", "last_assistant_message": "listo",
                "background_tasks": tasks })
    }

    #[tokio::test]
    async fn a_terminal_stays_running_while_its_sub_agents_work() {
        let state = running_task().await;
        let live = pty_session(&state, "s-1").await;

        post(&state, URL, terminal_stop(&["a1", "a2"])).await;
        assert!(live.has_held_stop());
        assert_eq!(live.state(), LiveState::Working);
        assert_eq!(status(&state).await, TaskStatus::Running);
        assert!(inbox(&state).await.is_empty());

        // The sub-agents' tools and Claude Code's idle reminder change nothing.
        for hook in [
            json!({ "hook_event_name": "PostToolUse", "tool_name": "Bash", "agent_id": "a1" }),
            json!({ "hook_event_name": "Notification", "notification_type": "idle_prompt",
                    "message": "Claude is waiting for your input" }),
        ] {
            post(&state, URL, hook).await;
            assert_eq!(live.state(), LiveState::Working);
            assert_eq!(status(&state).await, TaskStatus::Running);
        }
        assert!(inbox(&state).await.is_empty());

        // A sub-agent reporting back starts the next turn, whose Stop applies at once.
        post(
            &state,
            URL,
            json!({ "hook_event_name": "UserPromptSubmit",
                                  "prompt": "<agent-message from=\"a1\">done</agent-message>" }),
        )
        .await;
        assert!(!live.has_held_stop());
        post(&state, URL, terminal_stop(&[])).await;
        assert_eq!(live.state(), LiveState::Waiting);
        assert_eq!(status(&state).await, TaskStatus::WaitingForUser);
    }

    #[tokio::test]
    async fn a_terminal_stop_behind_background_shells_only_applies() {
        let state = running_task().await;
        let live = pty_session(&state, "s-1").await;
        let stop = json!({ "hook_event_name": "Stop", "background_tasks": [
            { "id": "b1", "type": "shell", "status": "running", "description": "dev server" }
        ] });
        post(&state, URL, stop).await;
        assert!(!live.has_held_stop());
        assert_eq!(status(&state).await, TaskStatus::WaitingForUser);
    }

    #[tokio::test]
    async fn a_task_notification_after_a_held_terminal_stop_is_a_new_turn() {
        let state = running_task().await;
        let live = pty_session(&state, "s-1").await;
        let notification = json!({ "hook_event_name": "UserPromptSubmit",
                                   "prompt": "<task-notification>\n<task-id>a1</task-id>" });
        // Mid-turn (nothing held) it is ignored, as before.
        post(&state, URL, notification.clone()).await;
        assert_eq!(live.state(), LiveState::Working);

        post(&state, URL, terminal_stop(&["a1"])).await;
        post(&state, URL, notification).await;
        assert!(!live.has_held_stop());
        post(&state, URL, terminal_stop(&[])).await;
        assert_eq!(status(&state).await, TaskStatus::WaitingForUser);
    }

    #[tokio::test]
    async fn a_held_terminal_stop_applies_once_its_sub_agents_end_with_no_new_turn() {
        let state = running_task().await;
        let live = pty_session(&state, "s-1").await;
        post(&state, URL, terminal_stop(&["a1", "a2"])).await;

        let ended = |id: &str| json!({ "hook_event_name": "SubagentStop", "agent_id": id });
        post(&state, URL, ended("a1")).await;
        tokio::time::sleep(std::time::Duration::from_millis(900)).await;
        assert!(live.has_held_stop(), "a2 still runs: no timer");
        assert_eq!(status(&state).await, TaskStatus::Running);

        post(&state, URL, ended("a2")).await;
        let mut waited = 0;
        while live.state() != LiveState::Waiting && waited < 50 {
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            waited += 1;
        }
        assert!(!live.has_held_stop());
        assert_eq!(live.state(), LiveState::Waiting);
        assert_eq!(status(&state).await, TaskStatus::WaitingForUser);
        assert_eq!(inbox(&state).await.len(), 1);
    }
}

#[cfg(test)]
mod background_tests {
    use std::sync::Arc;
    use std::time::Duration;

    use delilah_core::{Autopilot, AutopilotState, Step};

    use super::*;
    use crate::config::Config;
    use crate::pr_testkit::{TempDir, task_with_workspace};
    use crate::store_sessions::{SessionKind, SessionRecord};

    const RUNNING: &str = include_str!("../tests/fixtures/hooks/stop-background-running.json");
    const DONE: &str = include_str!("../tests/fixtures/hooks/stop-background-done.json");
    const EMPTY: &str = include_str!("../tests/fixtures/hooks/stop-background-empty.json");
    const NO_FIELD: &str = include_str!("../tests/fixtures/hooks/stop-no-background-field.json");

    /// What a test keeps alive: the task's folder (removed on drop) and the live session,
    /// whose `sleep` child is killed on drop so it does not outlive the test.
    struct Fixture {
        live: crate::sessions::Live,
        _dir: TempDir,
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = self.live.runtime.kill();
        }
    }

    /// A task on autopilot whose primary session `s-1` is on `step`; `tasks` is the
    /// content of its `tasks.md`, if any.
    async fn on_autopilot(step: Step, tasks: Option<&str>) -> (AppState, TaskKey, Fixture) {
        // The task, its workspace and its folder (removed on drop) come from the shared
        // fixture; a launch that tries to start the next step must fail fast, not run a
        // real agent.
        let (mut app, key, dir) =
            task_with_workspace(Arc::new(crate::gh::fake::FakeGh::new()), None).await;
        app.config = Arc::new(Config {
            claude_bin: "/nonexistent/claude".into(),
            ..(*app.config).clone()
        });
        app.store.set_saving_mode(&key, true).await.unwrap();
        let a = Autopilot {
            state: AutopilotState::Running,
            stage: None,
            reason: None,
            since: Utc::now(),
            nudged: None,
            review_session: None,
        };
        app.store.set_autopilot(&key, Some(&a)).await.unwrap();
        app.store
            .create_session(&SessionRecord {
                id: "s-1".into(),
                task: "PROJ-1".into(),
                kind: SessionKind::Claude,
                label: "Principal".into(),
                primary: true,
                closed: false,
                read_only: false,
                conversation: false,
                step: Some(step),
            })
            .await
            .unwrap();
        if let Some(tasks) = tasks {
            let spec = dir.path().join("ws/specs/proj-1-x");
            std::fs::create_dir_all(&spec).unwrap();
            std::fs::write(spec.join("tasks.md"), tasks).unwrap();
        }
        // The nudge types into the live session: a process that just sits there.
        let mut cmd = portable_pty::CommandBuilder::new("sleep");
        cmd.arg("30");
        let pty = delilah_agents::pty::PtySession::spawn(cmd, 80, 24, None).unwrap();
        let live = app
            .sessions
            .insert("s-1", "PROJ-1", SessionKind::Claude, true, pty);
        (app, key, Fixture { live, _dir: dir })
    }

    /// Posts a fixture through `apply_hook` as the primary session's turn end. The
    /// turn-end work is spawned by the hook, so this returns before it has run.
    async fn post(app: &AppState, fixture: &str) {
        let mut hook: Value = serde_json::from_str(fixture).unwrap();
        hook["session_id"] = "s-1".into();
        let q = HookQuery {
            task: "PROJ-1".into(),
            session: Some("s-1".into()),
        };
        assert_eq!(
            apply_hook(app.clone(), q, hook).await,
            StatusCode::NO_CONTENT
        );
    }

    /// The daemon's log, captured on this test's thread (the turn-end work runs on the
    /// same single-threaded runtime). Keep the guard alive for the whole test.
    fn capture_logs() -> (crate::logs::LogBuffer, tracing::subscriber::DefaultGuard) {
        let logs = crate::logs::LogBuffer::default();
        let subscriber = tracing_subscriber::fmt()
            .with_writer(logs.clone())
            .with_ansi(false)
            .with_max_level(tracing::Level::DEBUG)
            .finish();
        (logs, tracing::subscriber::set_default(subscriber))
    }

    /// How many turn ends the daemon skipped the stall for because of background work.
    /// This is the positive signal that the spawned turn-end work ran and chose to wait:
    /// a fixed sleep, or a count of live tasks, can say "done" before it has run at all.
    fn skipped(logs: &crate::logs::LogBuffer) -> usize {
        logs.tail(10_000)
            .iter()
            .filter(|l| l.contains(crate::handoff::BACKGROUND_WAIT_LOG))
            .count()
    }

    /// Polls (25 ms steps, up to 10 s) until `done` holds.
    async fn until<F, Fut>(what: &str, mut done: F)
    where
        F: FnMut() -> Fut,
        Fut: std::future::Future<Output = bool>,
    {
        for _ in 0..400 {
            if done().await {
                return;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        panic!("timed out waiting for {what}");
    }

    /// Posts a turn end with background work running and waits until the daemon has
    /// skipped its stall (the `n`th skip in `logs`).
    async fn background_stop(app: &AppState, logs: &crate::logs::LogBuffer, n: usize) {
        post(app, RUNNING).await;
        until("the stall to be skipped", || async { skipped(logs) >= n }).await;
    }

    /// Waits until `name` has been recorded `n` times for the task.
    async fn until_events(app: &AppState, key: &TaskKey, name: &str, n: usize) {
        until(name, || async { count(app, key, name).await >= n }).await;
    }

    async fn autopilot(app: &AppState, key: &TaskKey) -> Autopilot {
        let task = app.store.get_task(key).await.unwrap().unwrap();
        task.autopilot.unwrap()
    }

    async fn count(app: &AppState, key: &TaskKey, name: &str) -> usize {
        let events = app.store.events(key).await.unwrap();
        events.iter().filter(|e| e.event.name() == name).count()
    }

    async fn untouched(app: &AppState, key: &TaskKey) {
        let a = autopilot(app, key).await;
        assert_eq!(a.state, AutopilotState::Running, "{a:?}");
        assert_eq!(a.nudged, None);
        assert_eq!(count(app, key, "autopilot-nudged").await, 0);
        assert_eq!(count(app, key, "autopilot-paused").await, 0);
    }

    /// The step's one nudge was taken, once, and nothing paused.
    async fn nudged_once(app: &AppState, key: &TaskKey, step: Step) {
        let a = autopilot(app, key).await;
        assert_eq!(a.state, AutopilotState::Running, "{a:?}");
        assert_eq!(a.nudged, Some(step));
        assert_eq!(count(app, key, "autopilot-nudged").await, 1);
        assert_eq!(count(app, key, "autopilot-paused").await, 0);
    }

    #[tokio::test]
    async fn a_plan_turn_end_with_background_work_is_not_a_stall() {
        let (logs, _g) = capture_logs();
        let (app, key, _fx) = on_autopilot(Step::Plan, None).await;
        background_stop(&app, &logs, 1).await;
        untouched(&app, &key).await;
    }

    #[tokio::test]
    async fn repeated_background_turn_ends_never_nudge_or_pause() {
        let (logs, _g) = capture_logs();
        let (app, key, _fx) = on_autopilot(Step::Plan, None).await;
        for n in 1..=3 {
            background_stop(&app, &logs, n).await;
            untouched(&app, &key).await;
        }
    }

    #[tokio::test]
    async fn an_implement_turn_end_with_background_work_is_not_a_stall() {
        let (logs, _g) = capture_logs();
        let (app, key, _fx) = on_autopilot(Step::Implement, Some(TASKS)).await;
        for n in 1..=3 {
            background_stop(&app, &logs, n).await;
            untouched(&app, &key).await;
        }
    }

    #[tokio::test]
    async fn the_plan_handoff_still_starts_with_background_work_running() {
        let (app, key, _fx) = on_autopilot(Step::Plan, Some("- [ ] T001 One\n")).await;
        post(&app, RUNNING).await;
        // The handoff ran: the implementation's session exists (or the handoff is on record).
        until("the handoff to the implementation", || async {
            let sessions = app.store.open_sessions(Some(key.as_str())).await.unwrap();
            let task = app.store.get_task(&key).await.unwrap().unwrap();
            sessions.iter().any(|s| s.step == Some(Step::Implement))
                || task.handoff.is_some_and(|h| h.to == Step::Implement)
        })
        .await;
    }

    const TASKS: &str = "- [ ] T001 One\n- [ ] T002 Two\n";

    /// FR-003 / SC-002: a turn end that reports no background work running is a stall
    /// as before: one nudge, then a pause with the unchecked tasks as the reason.
    async fn nudges_then_pauses(fixture: &str) {
        let (app, key, _fx) = on_autopilot(Step::Implement, Some(TASKS)).await;
        post(&app, fixture).await;
        until_events(&app, &key, "autopilot-nudged", 1).await;
        nudged_once(&app, &key, Step::Implement).await;
        post(&app, fixture).await;
        until_events(&app, &key, "autopilot-paused", 1).await;
        let a = autopilot(&app, &key).await;
        assert_eq!(a.state, AutopilotState::Paused, "{a:?}");
        let reason = a.reason.unwrap();
        assert!(
            reason.contains("2 tasks unchecked") || reason.contains("2 tareas sin marcar"),
            "{reason}"
        );
        assert_eq!(count(&app, &key, "autopilot-nudged").await, 1);
        assert_eq!(count(&app, &key, "autopilot-paused").await, 1);
    }

    #[tokio::test]
    async fn all_background_work_finished_is_a_stall() {
        nudges_then_pauses(DONE).await;
    }

    #[tokio::test]
    async fn an_empty_background_list_is_a_stall() {
        nudges_then_pauses(EMPTY).await;
    }

    #[tokio::test]
    async fn no_background_field_is_a_stall() {
        nudges_then_pauses(NO_FIELD).await;
    }

    #[tokio::test]
    async fn waiting_on_background_work_does_not_spend_the_nudge() {
        let (logs, _g) = capture_logs();
        let (app, key, _fx) = on_autopilot(Step::Implement, Some(TASKS)).await;
        for n in 1..=3 {
            background_stop(&app, &logs, n).await;
        }
        untouched(&app, &key).await;
        post(&app, EMPTY).await;
        until_events(&app, &key, "autopilot-nudged", 1).await;
        nudged_once(&app, &key, Step::Implement).await;
    }

    #[tokio::test]
    async fn a_running_and_an_empty_turn_end_together_nudge_once() {
        let (logs, _g) = capture_logs();
        let (app, key, _fx) = on_autopilot(Step::Implement, Some(TASKS)).await;
        tokio::join!(post(&app, RUNNING), post(&app, EMPTY));
        // Both turn-end works have decided: one nudged, the other skipped its stall.
        until_events(&app, &key, "autopilot-nudged", 1).await;
        until("the stall to be skipped", || async { skipped(&logs) >= 1 }).await;
        nudged_once(&app, &key, Step::Implement).await;
    }

    #[tokio::test]
    async fn a_failed_turn_with_background_work_still_pauses() {
        let (app, key, _fx) = on_autopilot(Step::Implement, Some(TASKS)).await;
        let mut hook: Value = serde_json::from_str(RUNNING).unwrap();
        hook["hook_event_name"] = "StopFailure".into();
        let q = HookQuery {
            task: "PROJ-1".into(),
            session: Some("s-1".into()),
        };
        apply_hook(app.clone(), q, hook).await;
        until_events(&app, &key, "autopilot-paused", 1).await;
        let a = autopilot(&app, &key).await;
        assert_eq!(a.state, AutopilotState::Paused, "{a:?}");
        assert_eq!(count(&app, &key, "autopilot-nudged").await, 0);
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
        state.lang.set(crate::lang::Lang::Es); // Spanish fixtures
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
