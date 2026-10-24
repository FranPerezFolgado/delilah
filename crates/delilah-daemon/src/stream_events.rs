//! What a chat-mode (stream) session says, handled by one pump task per session.
//!
//! Task states (working, waiting…) still come from the hooks, which fire in stream mode
//! too; the pump only handles what the hooks do not carry. More arms (turn results,
//! permission requests) go in [`handle`].

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
    tokio::spawn(async move {
        loop {
            let event = match events.recv().await {
                Ok(event) => event,
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
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
            let exited = matches!(event, StreamEvent::Exited { .. });
            handle(&app, &task, &session, event);
            if exited {
                break;
            }
        }
    });
    started_rx
}

fn handle(app: &AppState, task: &str, _session: &str, event: StreamEvent) {
    if let StreamEvent::RateLimit {
        five_hour,
        seven_day,
    } = event
    {
        rate_limit(app, task, five_hour, seven_day);
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
