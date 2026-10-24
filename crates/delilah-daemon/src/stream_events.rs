//! What a chat-mode (stream) session says, handled by one pump task per session.
//!
//! Task states (working, waiting…) still come from the hooks, which fire in stream mode
//! too; the pump only handles what the hooks do not carry (the end of an interrupted
//! turn, rate limits). More arms (permission requests) go in [`handle`].

use delilah_agents::runtime::Runtime;
use delilah_agents::stream::{LimitWindow, StreamEvent};
use serde_json::{Map, Value, json};
use tokio::sync::{broadcast, oneshot};

use crate::state::AppState;

/// How the start of a stream session went, as the pump saw it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Started {
    /// The agent said hello: it takes messages now.
    Init,
    /// The process ended before that.
    Exited,
}

/// Starts the session's pump over its events (subscribed before anything was read).
/// The receiver resolves at the `Init` event, or when the process exits before it.
pub fn spawn_pump(
    app: &AppState,
    session: &str,
    task: &str,
    mut events: broadcast::Receiver<StreamEvent>,
) -> oneshot::Receiver<Started> {
    let (started_tx, started_rx) = oneshot::channel();
    let mut started_tx = Some(started_tx);
    let app = app.clone();
    let session = session.to_string();
    let task = task.to_string();
    // The process, to end the pump when it exits even if the `Exited` event was lost.
    let runtime = app.sessions.get(&session).map(|live| live.runtime);
    tokio::spawn(async move {
        // Once the process is gone, what it said last is still read; then the pump is done.
        let mut gone = false;
        loop {
            let event = if gone {
                events.try_recv().map_err(|e| match e {
                    broadcast::error::TryRecvError::Lagged(n) => {
                        broadcast::error::RecvError::Lagged(n)
                    }
                    _ => broadcast::error::RecvError::Closed,
                })
            } else {
                tokio::select! {
                    event = events.recv() => event,
                    () = process_exit(runtime.as_ref()) => {
                        gone = true;
                        continue;
                    }
                }
            };
            let event = match event {
                Ok(event) => event,
                Err(broadcast::error::RecvError::Lagged(missed)) => {
                    tracing::warn!(%session, missed, "the stream pump fell behind");
                    if runtime.as_ref().is_some_and(Runtime::has_exited) {
                        break;
                    }
                    continue;
                }
                Err(broadcast::error::RecvError::Closed) => break,
            };
            let outcome = match &event {
                StreamEvent::Init { .. } => Some(Started::Init),
                StreamEvent::Exited { .. } => Some(Started::Exited),
                _ => None,
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
        } => rate_limit(app, task, five_hour, seven_day),
        // No `Stop` hook fires after an interrupt: its result is what ends the turn.
        StreamEvent::TurnResult { subtype, .. } if subtype == "error_during_execution" => {
            if let Some(live) = app.sessions.get(session) {
                crate::conversation::turn_interrupted(app, &live).await;
            }
        }
        _ => {}
    }
}

/// The status line never runs in print mode: the stream's rate-limit events feed the
/// same usage state, in the same shape. A window the event leaves out keeps its last reading.
fn rate_limit(
    app: &AppState,
    task: &str,
    five_hour: Option<LimitWindow>,
    seven_day: Option<LimitWindow>,
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
    if !limits.is_empty() {
        crate::hooks::record_limits(app, task.to_string(), Value::Object(limits));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sessions::LiveState;
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

    #[tokio::test]
    async fn an_interrupted_turn_ends_with_its_result_and_the_session_waits() {
        let app = test_app().await;
        let live = insert_stream(&app, "sleep", &["5"]);
        live.set_state(LiveState::Working);
        live.set_activity(Some("Leyendo x".into()));
        let (tx, rx) = broadcast::channel(8);
        spawn_pump(&app, "s-1", "PROJ-1", rx);
        // A normal result changes nothing: the Stop hook ends that turn.
        tx.send(StreamEvent::TurnResult {
            subtype: "success".into(),
            is_error: false,
            result: Some("ok".into()),
        })
        .unwrap();
        tx.send(StreamEvent::TurnResult {
            subtype: "error_during_execution".into(),
            is_error: true,
            result: None,
        })
        .unwrap();
        for _ in 0..100 {
            if live.state() == LiveState::Waiting {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
        assert_eq!(live.state(), LiveState::Waiting);
        assert_eq!(live.activity(), None);
        let _ = live.runtime.kill();
    }

    #[tokio::test]
    async fn a_lost_exit_event_does_not_leave_the_pump_running() {
        let app = test_app().await;
        let live = insert_stream(&app, "true", &[]);
        live.runtime.wait().await;
        // A receiver that missed events, the `Exited` among them, with the sender kept.
        let (tx, rx) = broadcast::channel(2);
        for _ in 0..4 {
            let _ = tx.send(StreamEvent::TextDelta {
                message: "m".into(),
                text: "x".into(),
            });
        }
        let ended = spawn_pump(&app, "s-1", "PROJ-1", rx);
        // The pump dropped its end of the start signal (it never sent one) when it ended.
        let result = tokio::time::timeout(std::time::Duration::from_secs(5), ended)
            .await
            .expect("the pump ends");
        assert!(result.is_err());
        drop(tx);
    }

    #[tokio::test]
    async fn the_pump_ends_when_the_process_does_even_if_no_event_says_so() {
        let app = test_app().await;
        let live = insert_stream(&app, "true", &[]);
        let (tx, rx) = broadcast::channel::<StreamEvent>(8);
        let ended = spawn_pump(&app, "s-1", "PROJ-1", rx);
        live.runtime.wait().await;
        let result = tokio::time::timeout(std::time::Duration::from_secs(5), ended)
            .await
            .expect("the pump ends with the process");
        assert!(result.is_err(), "no start was ever signalled");
        drop(tx);
    }

    #[tokio::test]
    async fn a_rate_limit_event_feeds_the_usage_state_in_the_status_line_shape() {
        let app = AppState::new(
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
        );
        let (tx, rx) = broadcast::channel(8);
        let started = spawn_pump(&app, "s-1", "PROJ-1", rx);
        tx.send(StreamEvent::Init {
            session_id: "s-1".into(),
        })
        .unwrap();
        assert_eq!(started.await.unwrap(), Started::Init);
        tx.send(StreamEvent::RateLimit {
            five_hour: Some(LimitWindow {
                percent: 42,
                resets_at: 1792422000,
            }),
            seven_day: None,
        })
        .unwrap();
        for _ in 0..100 {
            if app.latest_limits().is_some() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
        let limits = app.latest_limits().unwrap();
        assert_eq!(limits["five_hour"]["used_percentage"], 42);
        assert_eq!(limits["five_hour"]["resets_at"], 1792422000);
    }
}
