//! What a chat-mode (stream) session says, handled by one pump task per session.
//!
//! Task states (working, waiting…) still come from the hooks, which fire in stream mode
//! too; the pump only handles what the hooks do not carry (the end of an interrupted
//! turn, rate limits). Permission requests go to `permissions.rs`.

use delilah_agents::runtime::Runtime;
use delilah_agents::stream::{LimitWindow, StreamEvent};
use serde_json::{Map, Value, json};
use tokio::sync::{mpsc, oneshot};

use crate::sessions::LiveState;
use crate::state::AppState;

/// How the start of a stream session went, as the pump saw it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Started {
    /// The agent is working on its first turn: its `Init`, or a message or result, came.
    /// (The real CLI says nothing but hook lines until it has read the first message.)
    Up,
    /// The process ended before that.
    Exited,
}

/// Starts the session's pump over its lossless events (see `StreamSession::take_events`:
/// every event but the text, none dropped). The receiver resolves at the first sign of
/// the agent working, or when the process exits before one.
pub fn spawn_pump(
    app: &AppState,
    session: &str,
    task: &str,
    mut events: mpsc::UnboundedReceiver<StreamEvent>,
) -> oneshot::Receiver<Started> {
    let (started_tx, started_rx) = oneshot::channel();
    let mut started_tx = Some(started_tx);
    let app = app.clone();
    let session = session.to_string();
    let task = task.to_string();
    // The process, to end the pump when it exits even if the `Exited` event never comes.
    let runtime = app.sessions.get(&session).map(|live| live.runtime);
    tokio::spawn(async move {
        // Once the process is gone, what it said last is still read; then the pump is done.
        let mut gone = false;
        loop {
            let event = if gone {
                events.try_recv().ok()
            } else {
                tokio::select! {
                    event = events.recv() => event,
                    () = process_exit(runtime.as_ref()) => {
                        gone = true;
                        continue;
                    }
                }
            };
            let Some(event) = event else { break };
            let outcome = match &event {
                StreamEvent::Exited { .. } => Some(Started::Exited),
                StreamEvent::RateLimit { .. } => None,
                _ => Some(Started::Up),
            };
            if let (Some(outcome), Some(tx)) = (outcome, started_tx.take()) {
                let _ = tx.send(outcome);
            }
            let ended = matches!(event, StreamEvent::Exited { .. });
            handle(&app, &task, &session, event).await;
            if ended {
                break;
            }
        }
    });
    started_rx
}

/// Resolves when the process is gone; never for a pump without a process to watch.
async fn process_exit(runtime: Option<&Runtime>) {
    match runtime {
        Some(runtime) => runtime.wait().await,
        None => std::future::pending().await,
    }
}

async fn handle(app: &AppState, task: &str, session: &str, event: StreamEvent) {
    match event {
        StreamEvent::RateLimit {
            five_hour,
            seven_day,
            reached,
            kind,
            resets_at,
        } => {
            rate_limit(
                app,
                task,
                five_hour,
                seven_day,
                reached.then_some((kind, resets_at)),
            );
            if reached {
                pause_on_limit(app, task, session).await;
            }
        }
        StreamEvent::TurnResult {
            subtype,
            is_error,
            result,
        } => turn_result(app, task, session, &subtype, is_error, result.as_deref()).await,
        // Answered later, from the API: the pump must not wait for the user.
        StreamEvent::PermissionRequest {
            request_id,
            tool,
            input,
            suggestion,
        } => {
            crate::permissions::on_request(app, task, session, request_id, tool, input, suggestion)
                .await;
        }
        // Nobody listens any more: what was asked of it cannot be answered.
        StreamEvent::Exited { .. } => {
            crate::permissions::release_session(app, session).await;
        }
        _ => {}
    }
}

/// How a turn ended. No `Stop` hook fires after an interrupt or any error that is not an
/// API one: the result is what ends the turn. A usage limit pauses the task instead.
async fn turn_result(
    app: &AppState,
    task: &str,
    session: &str,
    subtype: &str,
    is_error: bool,
    result: Option<&str>,
) {
    if is_error && mentions_limit(subtype, result) {
        pause_on_limit(app, task, session).await;
        return;
    }
    if subtype == "success" {
        return;
    }
    let Some(live) = app.sessions.get(session) else {
        return;
    };
    // The turn ended on the usage limit: the pause stays until the task is resumed.
    if live.state() == LiveState::Paused {
        return;
    }
    // An interrupt's result can come after a new message was sent: it is not that turn's.
    if subtype == "error_during_execution"
        && let Some(stream) = live.runtime.as_stream()
        && !stream.take_interrupt_current()
    {
        return;
    }
    crate::conversation::turn_interrupted(app, &live).await;
}

fn mentions_limit(subtype: &str, result: Option<&str>) -> bool {
    let text = format!("{subtype} {}", result.unwrap_or_default()).to_lowercase();
    [
        "usage limit",
        "rate limit",
        "rate_limit",
        "limit reached",
        "429",
    ]
    .iter()
    .any(|needle| text.contains(needle))
}

/// The stream's way to the pause the `StopFailure` hook gives a terminal session. Not
/// twice: a session already paused has been told.
async fn pause_on_limit(app: &AppState, task: &str, session: &str) {
    if app
        .sessions
        .get(session)
        .is_some_and(|live| live.state() == LiveState::Paused)
    {
        return;
    }
    crate::hooks::limit_hit_from_stream(app, task, session).await;
}

/// The status line never runs in print mode: the stream's rate-limit events feed the
/// same usage state, in the same shape. A window the event leaves out keeps its last
/// reading. When the limit is `reached`, the window it names is taken as full.
fn rate_limit(
    app: &AppState,
    task: &str,
    five_hour: Option<LimitWindow>,
    seven_day: Option<LimitWindow>,
    reached: Option<(Option<String>, Option<i64>)>,
) {
    let mut limits = match app.latest_limits() {
        Some(Value::Object(known)) => known,
        _ => Map::new(),
    };
    for (name, window) in [("five_hour", five_hour), ("seven_day", seven_day)] {
        if let Some(w) = window {
            limits.insert(
                name.into(),
                json!({ "used_percentage": w.percent, "resets_at": w.resets_at }),
            );
        }
    }
    if let Some((Some(kind), resets_at)) = reached
        && matches!(kind.as_str(), "five_hour" | "seven_day")
    {
        let resets_at = resets_at.or_else(|| {
            limits
                .get(&kind)
                .and_then(|w| w.get("resets_at"))
                .and_then(Value::as_i64)
        });
        limits.insert(
            kind,
            json!({ "used_percentage": 100, "resets_at": resets_at }),
        );
    }
    if !limits.is_empty() {
        crate::hooks::record_limits(app, task.to_string(), Value::Object(limits));
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use delilah_core::{Event, Task, TaskKey, TaskStatus};

    use super::*;
    use crate::store_sessions::SessionKind;

    async fn test_app() -> AppState {
        AppState::new(
            crate::store::Store::in_memory().await.unwrap(),
            crate::config::Config {
                home: std::env::temp_dir().join(format!("delilah-pump-{}", uuid::Uuid::new_v4())),
                addr: crate::config::DEFAULT_ADDR.parse().unwrap(),
                allow_file_protocol: false,
                notify: false,
                ui_origin: None,
                agent_program: None,
                claude_projects: crate::config::Config::scratch_projects(),
                claude_bin: "claude".into(),
            },
        )
    }

    /// A chat-mode session "s-1" of PROJ-1 whose process is `program args`.
    fn insert_stream(app: &AppState, program: &str, args: &[&str]) -> crate::sessions::Live {
        let mut cmd = tokio::process::Command::new(program);
        cmd.args(args);
        let (session, _rx) = delilah_agents::stream::StreamSession::spawn(cmd).unwrap();
        app.sessions.insert(
            "s-1",
            "PROJ-1",
            SessionKind::Claude,
            true,
            std::sync::Arc::new(session),
        )
    }

    /// PROJ-1 running, as a started chat-mode task is.
    async fn running_task(app: &AppState) -> TaskKey {
        let key = TaskKey::new("PROJ-1");
        app.store
            .create_task(&Task::new(key.clone(), "t", "feature"))
            .await
            .unwrap();
        for event in [Event::Start, Event::WorkspaceReady] {
            app.transition(&key, event).await.unwrap();
        }
        key
    }

    async fn until(mut done: impl FnMut() -> bool) {
        for _ in 0..150 {
            if done() {
                return;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }

    fn result(subtype: &str, is_error: bool, result: Option<&str>) -> StreamEvent {
        StreamEvent::TurnResult {
            subtype: subtype.into(),
            is_error,
            result: result.map(str::to_string),
        }
    }

    fn limit_event() -> StreamEvent {
        StreamEvent::RateLimit {
            five_hour: None,
            seven_day: None,
            reached: true,
            kind: Some("five_hour".into()),
            resets_at: Some(4_000_000_000),
        }
    }

    #[tokio::test]
    async fn an_interrupted_turn_ends_with_its_result_and_the_session_waits() {
        let app = test_app().await;
        let live = insert_stream(&app, "sleep", &["5"]);
        live.set_state(LiveState::Working);
        live.set_activity(Some("Leyendo x".into()));
        let (tx, rx) = mpsc::unbounded_channel();
        spawn_pump(&app, "s-1", "PROJ-1", rx);
        // A normal result changes nothing: the Stop hook ends that turn.
        tx.send(result("success", false, Some("ok"))).unwrap();
        tx.send(result("error_during_execution", true, None))
            .unwrap();
        until(|| live.state() == LiveState::Waiting).await;
        assert_eq!(live.state(), LiveState::Waiting);
        assert_eq!(live.activity(), None);
        let _ = live.runtime.kill();
    }

    #[tokio::test]
    async fn a_late_interrupt_result_does_not_end_a_turn_started_after_it() {
        let app = test_app().await;
        let live = insert_stream(&app, "sleep", &["5"]);
        let stream = live.runtime.as_stream().unwrap().clone();
        live.set_state(LiveState::Working);
        // Detener, then a new message (its prompt hook sets Working) before the result.
        stream.interrupt().await.unwrap();
        stream.send_user("otra cosa").await.unwrap();
        live.set_activity(Some("Leyendo y".into()));
        let (tx, rx) = mpsc::unbounded_channel();
        spawn_pump(&app, "s-1", "PROJ-1", rx);
        tx.send(result("error_during_execution", true, None))
            .unwrap();
        // A marker the pump handles after it: once seen, the late result was handled.
        tx.send(StreamEvent::RateLimit {
            five_hour: Some(LimitWindow {
                percent: 5,
                resets_at: 1,
            }),
            seven_day: None,
            reached: false,
            kind: None,
            resets_at: None,
        })
        .unwrap();
        until(|| app.latest_limits().is_some()).await;
        assert_eq!(live.state(), LiveState::Working, "the new turn goes on");
        assert_eq!(live.activity().as_deref(), Some("Leyendo y"));
        let _ = live.runtime.kill();
    }

    #[tokio::test]
    async fn other_errors_end_the_turn_too() {
        let app = test_app().await;
        let live = insert_stream(&app, "sleep", &["5"]);
        live.set_state(LiveState::Working);
        let (tx, rx) = mpsc::unbounded_channel();
        spawn_pump(&app, "s-1", "PROJ-1", rx);
        tx.send(result("error_max_turns", true, None)).unwrap();
        until(|| live.state() == LiveState::Waiting).await;
        assert_eq!(live.state(), LiveState::Waiting);
        let _ = live.runtime.kill();
    }

    #[tokio::test]
    async fn the_pump_ends_when_the_process_does_even_if_no_event_says_so() {
        let app = test_app().await;
        let live = insert_stream(&app, "true", &[]);
        let (tx, rx) = mpsc::unbounded_channel::<StreamEvent>();
        let ended = spawn_pump(&app, "s-1", "PROJ-1", rx);
        live.runtime.wait().await;
        let result = tokio::time::timeout(Duration::from_secs(5), ended)
            .await
            .expect("the pump ends with the process");
        assert!(result.is_err(), "no start was ever signalled");
        drop(tx);
    }

    #[tokio::test]
    async fn the_start_is_signalled_at_the_first_sign_of_work_or_at_the_exit() {
        let app = test_app().await;
        let (tx, rx) = mpsc::unbounded_channel();
        let started = spawn_pump(&app, "s-1", "PROJ-1", rx);
        tx.send(StreamEvent::Init {
            session_id: "s-1".into(),
        })
        .unwrap();
        assert_eq!(started.await.unwrap(), Started::Up);
        let (tx, rx) = mpsc::unbounded_channel();
        let started = spawn_pump(&app, "s-1", "PROJ-1", rx);
        tx.send(StreamEvent::Exited { code: Some(2) }).unwrap();
        assert_eq!(started.await.unwrap(), Started::Exited);
    }

    #[tokio::test]
    async fn a_rate_limit_event_feeds_the_usage_state_in_the_status_line_shape() {
        let app = test_app().await;
        let (tx, rx) = mpsc::unbounded_channel();
        spawn_pump(&app, "s-1", "PROJ-1", rx);
        tx.send(StreamEvent::RateLimit {
            five_hour: Some(LimitWindow {
                percent: 42,
                resets_at: 1792422000,
            }),
            seven_day: None,
            reached: false,
            kind: None,
            resets_at: None,
        })
        .unwrap();
        until(|| app.latest_limits().is_some()).await;
        let limits = app.latest_limits().unwrap();
        assert_eq!(limits["five_hour"]["used_percentage"], 42);
        assert_eq!(limits["five_hour"]["resets_at"], 1792422000);
    }

    async fn paused(app: &AppState, key: &TaskKey) -> bool {
        matches!(
            app.store.get_task(key).await.unwrap().unwrap().status,
            TaskStatus::PausedByLimit { .. }
        )
    }

    #[tokio::test]
    async fn a_rate_limit_event_that_reaches_the_limit_pauses_the_task_once() {
        let app = test_app().await;
        let key = running_task(&app).await;
        let live = insert_stream(&app, "sleep", &["5"]);
        live.set_state(LiveState::Working);
        let (tx, rx) = mpsc::unbounded_channel();
        spawn_pump(&app, "s-1", "PROJ-1", rx);
        tx.send(limit_event()).unwrap();
        // The error result that follows the event, and the hook if it fires too.
        tx.send(result(
            "success",
            true,
            Some("You have hit your usage limit"),
        ))
        .unwrap();
        tx.send(result("error_during_execution", true, None))
            .unwrap();
        for _ in 0..150 {
            if paused(&app, &key).await {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        assert!(paused(&app, &key).await);
        assert_eq!(live.state(), LiveState::Paused);
        tokio::time::sleep(Duration::from_millis(200)).await;
        let limits = app
            .store
            .events(&key)
            .await
            .unwrap()
            .iter()
            .filter(|e| e.event.name() == "limit-reached")
            .count();
        assert_eq!(limits, 1);
        assert_eq!(live.state(), LiveState::Paused, "still paused, not Waiting");
        let _ = live.runtime.kill();
    }

    #[tokio::test]
    async fn an_error_result_that_mentions_the_usage_limit_pauses_the_task() {
        let app = test_app().await;
        let key = running_task(&app).await;
        let live = insert_stream(&app, "sleep", &["5"]);
        live.set_state(LiveState::Working);
        let (tx, rx) = mpsc::unbounded_channel();
        spawn_pump(&app, "s-1", "PROJ-1", rx);
        tx.send(result(
            "success",
            true,
            Some("Claude usage limit reached|1792872000"),
        ))
        .unwrap();
        for _ in 0..150 {
            if paused(&app, &key).await {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        assert!(paused(&app, &key).await);
        let _ = live.runtime.kill();
    }
}
