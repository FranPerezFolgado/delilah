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
    if !is_owner_name(&q.task) {
        return StatusCode::BAD_REQUEST;
    }
    // Kept raw: the exact payloads (e.g. a real StopFailure) are what the next
    // iteration of the adapter is built from.
    if let Err(e) = append_raw(&app, &q.task, &hook).await {
        tracing::warn!(task = %q.task, "cannot log hook: {e}");
    }

    let signal = claude::classify(&hook);
    let live = match &q.session {
        Some(id) => app.sessions.get(id),
        None => app.sessions.primary(&q.task),
    };
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
    StatusCode::NO_CONTENT
}

pub async fn claude_statusline(
    State(app): State<AppState>,
    Query(q): Query<HookQuery>,
    Json(payload): Json<Value>,
) -> StatusCode {
    if let Some(limits) = payload.get("rate_limits").filter(|v| v.is_object()) {
        // The status line runs every few seconds: only a new reading is news.
        let changed = {
            let mut usage = app.usage.lock().unwrap();
            let changed = usage.as_ref().is_none_or(|u| &u.rate_limits != limits);
            *usage = Some(UsageReport {
                at: Utc::now(),
                task: q.task,
                rate_limits: limits.clone(),
            });
            changed
        };
        if changed {
            app.emit(Topic::Usage);
        }
    }
    StatusCode::NO_CONTENT
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
