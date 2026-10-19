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

use crate::state::{AppState, UsageReport};
use crate::store::StoreError;

#[derive(Deserialize)]
pub struct HookQuery {
    task: String,
}

pub async fn claude_hook(
    State(app): State<AppState>,
    Query(q): Query<HookQuery>,
    Json(hook): Json<Value>,
) -> StatusCode {
    // Kept raw: the exact payloads (e.g. a real StopFailure) are what the next
    // iteration of the adapter is built from.
    if let Err(e) = append_raw(&app, &q.task, &hook).await {
        tracing::warn!(task = %q.task, "cannot log hook: {e}");
    }

    let signal = claude::classify(&hook);
    if signal == Signal::SessionStarted
        && let Some(live) = app.sessions.get(&q.task)
    {
        live.started.notify_one();
    }
    let event = match signal {
        Signal::PromptSubmitted | Signal::Working => Some(Event::UserAnswered),
        Signal::NeedsUser => Some(Event::AgentAsked),
        Signal::TurnEnded | Signal::TurnFailed => {
            let limits = app.latest_limits();
            match limits::limit_reset(signal, &hook, limits.as_ref(), Utc::now()) {
                Some(resume_at) => {
                    tracing::info!(task = %q.task, %resume_at, "turn ended on the usage limit");
                    Some(Event::LimitReached { resume_at })
                }
                None => Some(Event::AgentAsked),
            }
        }
        Signal::SessionStarted | Signal::SessionEnded | Signal::Other => None,
    };
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
        *app.usage.lock().unwrap() = Some(UsageReport {
            at: Utc::now(),
            task: q.task,
            rate_limits: limits.clone(),
        });
    }
    StatusCode::NO_CONTENT
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

    async fn status(state: &AppState) -> TaskStatus {
        state
            .store
            .get_task(&TaskKey::new("PROJ-1"))
            .await
            .unwrap()
            .unwrap()
            .status
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
