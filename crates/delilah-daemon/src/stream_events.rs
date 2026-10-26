//! What a chat-mode (stream) session says, handled by one pump task per session.
//!
//! Task states (working, waiting…) still come from the hooks, which fire in stream mode
//! too; the pump only handles what the hooks do not carry (the end of an interrupted
//! turn, rate limits). Permission requests go to `permissions.rs`.

use chrono::Utc;
use delilah_agents::runtime::Runtime;
use delilah_agents::stream::{LimitWindow, StreamEvent};
use serde_json::{Map, Value, json};
use tokio::sync::{mpsc, oneshot};

use delilah_core::{Event, TaskKey};

use crate::progress::{self, Progress};
use crate::sessions::LiveState;
use crate::state::AppState;

/// How the start of a stream session went, as the pump saw it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Started {
    /// The agent is working on its first turn: its `Init`, or a message or result, came.
    /// (The real CLI says nothing but hook lines until it has read the first message.)
    Up,
    /// The process ended before that.
    Exited,
    /// The first thing it said was an error result (a login or auth error, say): its text.
    Failed(String),
}

/// How long, once the process is gone, the pump waits for what it said last to arrive.
const DRAIN: std::time::Duration = std::time::Duration::from_millis(200);

/// How often, while finished background rows wait to expire, the progress is ticked.
#[cfg(not(test))]
const TICK: std::time::Duration = std::time::Duration::from_secs(5);
#[cfg(test)]
const TICK: std::time::Duration = std::time::Duration::from_millis(50);

/// How long, once the last background item ended with a Stop held, Claude has to start a
/// follow-up turn before the Stop applies as a normal end of turn.
#[cfg(not(test))]
const FALLBACK: std::time::Duration = std::time::Duration::from_secs(30);
#[cfg(test)]
const FALLBACK: std::time::Duration = std::time::Duration::from_millis(300);

/// Replays the Stop held behind background work as a normal end of turn. Call only once
/// the background set is empty (or its rows were marked stopped). The take is the single
/// claim: whoever gets the payload replays it, nobody twice.
pub async fn replay_held_stop(app: &AppState, task: &str, session: &str, held: Option<Value>) {
    if let Some(held) = held {
        crate::hooks::apply_signal(
            app.clone(),
            task,
            Some(session),
            delilah_agents::claude::Signal::TurnEnded,
            &held,
        )
        .await;
    }
}

/// A stream process is gone: its running rows become stopped and the Stop held behind them
/// applies first, as after a normal turn end, before the exit is handled.
pub async fn on_process_exit(app: &AppState, live: &crate::sessions::Live) {
    live.update_progress(|p| progress::apply(p, &StreamEvent::Exited { code: None }, Utc::now()));
    let held = live.take_held_stop();
    // A newer process on the same id owns the session now: its state is not touched.
    // Not in the registry at all (already removed) the held Stop still applies.
    if app
        .sessions
        .get(&live.id)
        .is_some_and(|current| !current.runtime.same(&live.runtime))
    {
        return;
    }
    replay_held_stop(app, &live.task, &live.id, held).await;
}

/// Whether a finished row or sub-agent still waits to expire from the progress.
fn final_rows_wait(p: &Progress) -> bool {
    p.background.iter().any(|b| b.state != "running")
        || p.subagents.iter().any(|s| s.state != "running")
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
    let live = app.sessions.get(&session);
    let runtime = live.as_ref().map(|live| live.runtime.clone());
    tokio::spawn(async move {
        // Once the process is gone, what it said last is still read; then the pump is done.
        let mut gone = false;
        // Whether the agent has shown any sign of work yet.
        let mut worked = false;
        // Ticks the progress so finished rows expire; it only runs while some wait.
        let mut ticker = tokio::time::interval(TICK);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        let mut ticking = false;
        let mut exit_applied = false;
        loop {
            let waiting = live
                .as_ref()
                .is_some_and(|live| final_rows_wait(&live.progress()));
            if waiting && !ticking {
                ticker.reset();
            }
            ticking = waiting;
            let event = if gone {
                tokio::time::timeout(DRAIN, events.recv())
                    .await
                    .ok()
                    .flatten()
            } else {
                tokio::select! {
                    event = events.recv() => event,
                    () = process_exit(runtime.as_ref()) => {
                        gone = true;
                        continue;
                    }
                    _ = ticker.tick(), if waiting => {
                        if let Some(live) = &live {
                            live.update_progress(|p| progress::tick(p, Utc::now()));
                        }
                        continue;
                    }
                }
            };
            let Some(event) = event else { break };
            if let Some(live) = &live {
                let before = live.progress().has_background();
                live.update_progress(|p| progress::apply(p, &event, Utc::now()));
                if matches!(event, StreamEvent::Init { .. }) {
                    // A turn began: the held Stop (and its timer) belong to the last one.
                    live.cancel_held_stop();
                } else if before
                    && !live.progress().has_background()
                    && !matches!(event, StreamEvent::Exited { .. })
                    && live.has_held_stop()
                {
                    arm_fallback(&app, live.clone(), &task);
                }
            }
            exit_applied |= matches!(event, StreamEvent::Exited { .. });
            let outcome = match &event {
                StreamEvent::Exited { .. } => Some(Started::Exited),
                StreamEvent::RateLimit { .. } => None,
                // An error before any sign of work is the start failing, not a turn.
                StreamEvent::TurnResult {
                    is_error: true,
                    result,
                    subtype,
                } if !worked => Some(Started::Failed(
                    result.clone().unwrap_or_else(|| subtype.clone()),
                )),
                _ => Some(Started::Up),
            };
            let failed = matches!(outcome, Some(Started::Failed(_)));
            if !matches!(
                event,
                StreamEvent::RateLimit { .. } | StreamEvent::Exited { .. }
            ) && !failed
            {
                worked = true;
            }
            let mut told = false;
            if let (Some(outcome), Some(tx)) = (outcome, started_tx.take()) {
                told = tx.send(outcome).is_ok();
            }
            let ended = matches!(event, StreamEvent::Exited { .. });
            // The start failing is undone by whoever waits for it: the error is not a turn.
            if !(failed && told) {
                handle(&app, &task, &session, runtime.as_ref(), event).await;
            }
            if ended {
                break;
            }
        }
        // The process is gone even if no `Exited` said so: its running work is over.
        if !exit_applied && owns(&app, &session, runtime.as_ref()) {
            if let Some(live) = &live {
                live.update_progress(|p| {
                    progress::apply(p, &StreamEvent::Exited { code: None }, Utc::now())
                });
            }
        }
        // However it ended, nobody is left to answer what the agent asked (unless the
        // session already runs a new process, whose asks are not this pump's).
        if owns(&app, &session, runtime.as_ref()) {
            crate::permissions::release_session(&app, &session).await;
        }
    });
    started_rx
}

/// The last background item ended with a Stop held: if no turn begins within `FALLBACK`,
/// the Stop applies as a normal end of turn (waiting, Bandeja, autopilot).
fn arm_fallback(app: &AppState, live: crate::sessions::Live, task: &str) {
    let token = live.arm_fallback();
    let app = app.clone();
    let task = task.to_string();
    tokio::spawn(async move {
        tokio::time::sleep(FALLBACK).await;
        let held = live.take_held_stop_unless_cancelled(&token);
        replay_held_stop(&app, &task, &live.id, held).await;
    });
}

/// Resolves when the process is gone; never for a pump without a process to watch.
async fn process_exit(runtime: Option<&Runtime>) {
    match runtime {
        Some(runtime) => runtime.wait().await,
        None => std::future::pending().await,
    }
}

/// Whether the session still runs the pump's process (or has none): after a runtime
/// switch or a relaunch on the same id, a late event of the old process must neither
/// deny the new one's asks nor raise an ask answered to the wrong process.
fn owns(app: &AppState, session: &str, runtime: Option<&Runtime>) -> bool {
    match (runtime, app.sessions.get(session)) {
        (Some(mine), Some(live)) => live.runtime.same(mine),
        _ => true,
    }
}

async fn handle(
    app: &AppState,
    task: &str,
    session: &str,
    runtime: Option<&Runtime>,
    event: StreamEvent,
) {
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
        StreamEvent::Init {
            self_started: true, ..
        } => self_started_turn(app, task, session).await,
        // Answered later, from the API: the pump must not wait for the user.
        StreamEvent::PermissionRequest {
            request_id,
            tool,
            input,
            suggestion,
        } if owns(app, session, runtime) => {
            crate::permissions::on_request(app, task, session, request_id, tool, input, suggestion)
                .await;
        }
        // Nobody listens any more: what was asked of it cannot be answered.
        StreamEvent::Exited { .. } if owns(app, session, runtime) => {
            crate::permissions::release_session(app, session).await;
        }
        _ => {}
    }
}

/// Claude began a turn without a user message (a background task ended, say). Only a
/// primary session that waits for the user is moved: it goes to working, the task back to
/// running and the previous turn's question is resolved, as a typed message does, through
/// the prompt hook the CLI also posts for such a turn (which then changes nothing more).
async fn self_started_turn(app: &AppState, task: &str, session: &str) {
    let Some(live) = app.sessions.get(session) else {
        return;
    };
    if !live.primary {
        return;
    }
    // Waiting: the real prompt hook has not come yet. If it came first, the session is
    // already working and has nothing left to move.
    if live.state() == LiveState::Waiting {
        let hook = json!({ "hook_event_name": "UserPromptSubmit", "delilah_origin": "agent" });
        crate::hooks::apply_signal(
            app.clone(),
            task,
            Some(session),
            delilah_agents::claude::Signal::PromptSubmitted,
            &hook,
        )
        .await;
    }
    // One `Init` per turn: the history entry is written here and only here.
    // A code, not the text: the UI words it.
    let reason = live
        .progress()
        .turn
        .and_then(|t| t.reason)
        .map(|_| "background-done".to_string());
    if let Err(e) = app
        .transition(&TaskKey::new(task), Event::AgentResumed { reason })
        .await
    {
        tracing::warn!(task, "cannot record the self-started turn: {e}");
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
    // Any result settles an interrupt asked before it: a marker left from one that ended
    // some other way must not make a later genuine error be taken for a stale one.
    let current = app
        .sessions
        .get(session)
        .and_then(|live| live.runtime.as_stream().map(|s| s.take_interrupt_current()))
        .unwrap_or(true);
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
    if subtype == "error_during_execution" && !current {
        return;
    }
    crate::conversation::turn_interrupted(app, &live).await;
}

/// Whether an error result says the usage or rate limit was hit. A bare `429` in free
/// text is not enough (a URL, a count, a line number can carry it): only the words, or
/// the status code when it leads the text as an API error does (`429 {…}`, `API Error: 429`).
fn mentions_limit(subtype: &str, result: Option<&str>) -> bool {
    let text = format!("{subtype} {}", result.unwrap_or_default()).to_lowercase();
    if ["usage limit", "rate limit", "rate_limit", "limit reached"]
        .iter()
        .any(|needle| text.contains(needle))
    {
        return true;
    }
    let result = result.unwrap_or_default().trim().to_lowercase();
    let api_status = result
        .strip_prefix("api error:")
        .map_or(result.as_str(), str::trim_start);
    api_status
        .strip_prefix("429")
        .is_some_and(|rest| !rest.starts_with(|c: char| c.is_ascii_alphanumeric()))
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
                ..crate::config::Config::test_defaults()
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

    fn bg_started(id: &str) -> StreamEvent {
        StreamEvent::Task(delilah_agents::stream::TaskEvent::Started {
            task_id: id.into(),
            tool_use_id: None,
            kind: delilah_agents::stream::TaskKind::Command,
            description: "sleep".into(),
            background: true,
        })
    }

    fn bg_ended(id: &str) -> StreamEvent {
        StreamEvent::Task(delilah_agents::stream::TaskEvent::Ended {
            task_id: id.into(),
            status: delilah_agents::stream::TaskStatus::Completed,
            summary: None,
        })
    }

    #[tokio::test]
    async fn every_event_feeds_the_progress() {
        let app = test_app().await;
        let live = insert_stream(&app, "sleep", &["5"]);
        let (tx, rx) = mpsc::unbounded_channel();
        spawn_pump(&app, "s-1", "PROJ-1", rx);
        tx.send(bg_started("b1")).unwrap();
        until(|| live.progress().has_background()).await;
        assert!(live.progress().has_background());
        tx.send(bg_ended("b1")).unwrap();
        until(|| !live.progress().has_background()).await;
        assert!(!live.progress().has_background());
        assert_eq!(live.progress().background[0].state, "completed");
        let _ = live.runtime.kill();
    }

    #[tokio::test]
    async fn a_tick_drops_final_rows_once_they_are_old() {
        let app = test_app().await;
        let live = insert_stream(&app, "sleep", &["300"]);
        let (tx, rx) = mpsc::unbounded_channel();
        spawn_pump(&app, "s-1", "PROJ-1", rx);
        tx.send(bg_started("b1")).unwrap();
        tx.send(bg_ended("b1")).unwrap();
        until(|| live.progress().background.len() == 1).await;
        assert_eq!(live.progress().background.len(), 1);
        // The tick judges ages by the wall clock: make the row old.
        live.update_progress(|p| {
            p.background[0].ended_at = Some(chrono::Utc::now() - chrono::Duration::seconds(120));
            true
        });
        until(|| live.progress().background.is_empty()).await;
        assert!(live.progress().background.is_empty());
        let _ = live.runtime.kill();
    }

    #[tokio::test]
    async fn the_pump_marks_running_rows_stopped_when_the_process_exits() {
        let app = test_app().await;
        let live = insert_stream(&app, "sleep", &["5"]);
        let (tx, rx) = mpsc::unbounded_channel();
        spawn_pump(&app, "s-1", "PROJ-1", rx);
        tx.send(bg_started("b1")).unwrap();
        until(|| live.progress().has_background()).await;
        tx.send(StreamEvent::Exited { code: Some(0) }).unwrap();
        until(|| !live.progress().has_background()).await;
        assert!(!live.progress().has_background());
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
            self_started: false,
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

    #[tokio::test]
    async fn a_stale_interrupt_marker_does_not_hide_a_later_genuine_error() {
        let app = test_app().await;
        let live = insert_stream(&app, "sleep", &["5"]);
        let stream = live.runtime.as_stream().unwrap().clone();
        live.set_state(LiveState::Working);
        // Detener, but the turn ended on its own: a success result, no interrupt result.
        stream.interrupt().await.unwrap();
        let (tx, rx) = mpsc::unbounded_channel();
        spawn_pump(&app, "s-1", "PROJ-1", rx);
        tx.send(result("success", false, Some("done"))).unwrap();
        // A new turn later fails for real.
        stream.send_user("otra cosa").await.unwrap();
        tx.send(result("error_during_execution", true, None))
            .unwrap();
        until(|| live.state() == LiveState::Waiting).await;
        assert_eq!(live.state(), LiveState::Waiting, "the error ends the turn");
        let _ = live.runtime.kill();
    }

    #[tokio::test]
    async fn an_error_result_before_any_work_is_a_failed_start_with_its_text() {
        let app = test_app().await;
        let (tx, rx) = mpsc::unbounded_channel();
        let started = spawn_pump(&app, "s-1", "PROJ-1", rx);
        tx.send(result("success", true, Some("Invalid API key")))
            .unwrap();
        assert_eq!(
            started.await.unwrap(),
            Started::Failed("Invalid API key".into())
        );
        // After a sign of work, an error is just a turn's end.
        let (tx, rx) = mpsc::unbounded_channel();
        let started = spawn_pump(&app, "s-1", "PROJ-1", rx);
        tx.send(StreamEvent::Init {
            session_id: "s-1".into(),
            self_started: false,
        })
        .unwrap();
        assert_eq!(started.await.unwrap(), Started::Up);
        tx.send(result("success", true, Some("boom"))).unwrap();
    }

    #[tokio::test]
    async fn what_the_agent_said_last_is_read_after_it_is_gone_and_its_asks_are_released() {
        let app = test_app().await;
        running_task(&app).await;
        let live = insert_stream(&app, "sh", &["-c", "sleep 0.3"]);
        let (tx, rx) = mpsc::unbounded_channel();
        spawn_pump(&app, "s-1", "PROJ-1", rx);
        tx.send(StreamEvent::PermissionRequest {
            request_id: "r-1".into(),
            tool: "Bash".into(),
            input: json!({ "command": "ls" }),
            suggestion: None,
        })
        .unwrap();
        for _ in 0..150 {
            if app.permissions.has_session("s-1") {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        assert!(app.permissions.has_session("s-1"));
        live.runtime.wait().await;
        // Said a moment after the process was seen gone, still within the drain.
        tokio::time::sleep(Duration::from_millis(60)).await;
        tx.send(StreamEvent::RateLimit {
            five_hour: Some(LimitWindow {
                percent: 7,
                resets_at: 1,
            }),
            seven_day: None,
            reached: false,
            kind: None,
            resets_at: None,
        })
        .unwrap();
        until(|| app.latest_limits().is_some()).await;
        assert!(app.latest_limits().is_some(), "read after the exit");
        // No `Exited` ever comes, and the pump still lets the asks go.
        for _ in 0..150 {
            if !app.permissions.has_session("s-1") {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        assert!(!app.permissions.has_session("s-1"));
        drop(tx);
    }

    #[tokio::test]
    async fn a_stream_limit_pause_is_not_written_to_the_raw_hooks_log() {
        let app = test_app().await;
        let key = running_task(&app).await;
        let live = insert_stream(&app, "sleep", &["5"]);
        live.set_state(LiveState::Working);
        let (tx, rx) = mpsc::unbounded_channel();
        spawn_pump(&app, "s-1", "PROJ-1", rx);
        tx.send(limit_event()).unwrap();
        for _ in 0..150 {
            if paused(&app, &key).await {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        assert!(paused(&app, &key).await);
        assert!(
            !app.config.task_dir("PROJ-1").join("hooks.jsonl").exists(),
            "hooks.jsonl holds what the agent posted, not what the stream implied"
        );
        let _ = live.runtime.kill();
    }

    #[test]
    fn only_the_limit_words_or_a_leading_429_mean_the_limit() {
        assert!(mentions_limit(
            "success",
            Some("Claude usage limit reached|17")
        ));
        assert!(mentions_limit("success", Some("Rate limit exceeded")));
        assert!(mentions_limit(
            "success",
            Some("API Error: 429 too many requests")
        ));
        assert!(mentions_limit("success", Some("429 {\"type\":\"error\"}")));
        assert!(!mentions_limit(
            "success",
            Some("fetch https://x.test/item/4291 failed")
        ));
        assert!(!mentions_limit(
            "success",
            Some("processed 429 files, then crashed")
        ));
        assert!(!mentions_limit("error_during_execution", None));
    }

    fn init(self_started: bool) -> StreamEvent {
        StreamEvent::Init {
            session_id: "c-1".into(),
            self_started,
        }
    }

    /// PROJ-1 waiting for the user after a turn that asked a question, session "s-1" waiting.
    async fn waiting_task(app: &AppState, live: &crate::sessions::Live) -> TaskKey {
        let key = running_task(app).await;
        app.store
            .create_session(&crate::store_sessions::SessionRecord {
                id: "s-1".into(),
                task: "PROJ-1".into(),
                kind: SessionKind::Claude,
                label: "s-1".into(),
                primary: true,
                closed: false,
                read_only: false,
                conversation: false,
                step: None,
            })
            .await
            .unwrap();
        app.transition(&key, Event::AgentAsked).await.unwrap();
        let question = crate::inbox::Attention {
            kind: crate::inbox::AttentionKind::Question,
            text: "¿sigo?".into(),
            since: Utc::now(),
        };
        app.store
            .set_attention("s-1", Some(&question))
            .await
            .unwrap();
        live.set_state(LiveState::Waiting);
        key
    }

    async fn status(app: &AppState, key: &TaskKey) -> TaskStatus {
        app.store.get_task(key).await.unwrap().unwrap().status
    }

    #[tokio::test]
    async fn a_self_started_turn_puts_a_waiting_session_and_task_to_work() {
        let app = test_app().await;
        let live = insert_stream(&app, "sleep", &["5"]);
        let key = waiting_task(&app, &live).await;
        let (tx, rx) = mpsc::unbounded_channel();
        spawn_pump(&app, "s-1", "PROJ-1", rx);
        tx.send(init(true)).unwrap();
        until(|| live.state() == LiveState::Working).await;
        assert_eq!(live.state(), LiveState::Working);
        for _ in 0..100 {
            if status(&app, &key).await == TaskStatus::Running {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        assert_eq!(status(&app, &key).await, TaskStatus::Running);
        assert!(app.store.attention("s-1").await.unwrap().is_none());
        for _ in 0..100 {
            if !resumed(&app, &key).await.is_empty() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        assert_eq!(resumed(&app, &key).await, vec![None]);
        let _ = live.runtime.kill();
    }

    /// The `reason` of every `agent-resumed` in the task's history.
    async fn resumed(app: &AppState, key: &TaskKey) -> Vec<Option<String>> {
        let events = app.store.events(key).await.unwrap();
        events
            .into_iter()
            .filter_map(|e| match e.event {
                Event::AgentResumed { reason } => Some(reason),
                _ => None,
            })
            .collect()
    }

    #[tokio::test]
    async fn a_self_started_turn_after_a_task_end_records_why() {
        let app = test_app().await;
        let live = insert_stream(&app, "sleep", &["5"]);
        let key = waiting_task(&app, &live).await;
        let (tx, rx) = mpsc::unbounded_channel();
        spawn_pump(&app, "s-1", "PROJ-1", rx);
        tx.send(bg_started("b1")).unwrap();
        tx.send(bg_ended("b1")).unwrap();
        tx.send(init(true)).unwrap();
        for _ in 0..100 {
            if !resumed(&app, &key).await.is_empty() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        assert_eq!(
            resumed(&app, &key).await,
            vec![Some("background-done".to_string())]
        );
        let _ = live.runtime.kill();
    }

    fn stop_hook() -> Value {
        json!({ "hook_event_name": "Stop", "last_assistant_message": "listo" })
    }

    /// A working session with one background item running and a Stop held behind it.
    async fn held_behind_background(
        tx: &mpsc::UnboundedSender<StreamEvent>,
        live: &crate::sessions::Live,
    ) {
        live.set_state(LiveState::Working);
        tx.send(bg_started("b1")).unwrap();
        until(|| live.progress().has_background()).await;
        assert!(live.hold_stop_if_background(stop_hook()));
    }

    #[tokio::test]
    async fn a_held_stop_is_replayed_when_no_turn_follows_the_last_background_item() {
        let app = test_app().await;
        let live = insert_stream(&app, "sleep", &["5"]);
        let key = running_task(&app).await;
        let (tx, rx) = mpsc::unbounded_channel();
        spawn_pump(&app, "s-1", "PROJ-1", rx);
        held_behind_background(&tx, &live).await;
        tx.send(bg_ended("b1")).unwrap();
        until(|| live.state() == LiveState::Waiting).await;
        assert_eq!(live.state(), LiveState::Waiting);
        assert!(!live.has_held_stop());
        for _ in 0..100 {
            if status(&app, &key).await == TaskStatus::WaitingForUser {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        assert_eq!(status(&app, &key).await, TaskStatus::WaitingForUser);
        let _ = live.runtime.kill();
    }

    #[tokio::test]
    async fn a_process_exit_replays_the_held_stop_even_if_the_session_left_the_registry() {
        let app = test_app().await;
        let live = insert_stream(&app, "sleep", &["5"]);
        let key = running_task(&app).await;
        let (tx, rx) = mpsc::unbounded_channel();
        spawn_pump(&app, "s-1", "PROJ-1", rx);
        held_behind_background(&tx, &live).await;
        assert!(live.has_held_stop());
        app.sessions.remove_if_same("s-1", &live.runtime);
        assert!(app.sessions.get("s-1").is_none());
        on_process_exit(&app, &live).await;
        assert!(!live.has_held_stop());
        assert_eq!(status(&app, &key).await, TaskStatus::WaitingForUser);
        let _ = live.runtime.kill();
    }

    #[tokio::test]
    async fn a_new_turn_before_the_fallback_drops_the_held_stop_and_its_timer() {
        let app = test_app().await;
        let live = insert_stream(&app, "sleep", &["5"]);
        let key = running_task(&app).await;
        let (tx, rx) = mpsc::unbounded_channel();
        spawn_pump(&app, "s-1", "PROJ-1", rx);
        held_behind_background(&tx, &live).await;
        tx.send(bg_ended("b1")).unwrap();
        tx.send(init(false)).unwrap();
        until(|| !live.has_held_stop()).await;
        assert!(!live.has_held_stop());
        tokio::time::sleep(FALLBACK * 3).await;
        assert_eq!(live.state(), LiveState::Working, "the timer did not fire");
        assert_eq!(status(&app, &key).await, TaskStatus::Running);
        let _ = live.runtime.kill();
    }

    #[tokio::test]
    async fn a_self_started_turn_with_a_held_stop_drops_it_and_stays_working() {
        let app = test_app().await;
        let live = insert_stream(&app, "sleep", &["5"]);
        let key = running_task(&app).await;
        let (tx, rx) = mpsc::unbounded_channel();
        spawn_pump(&app, "s-1", "PROJ-1", rx);
        held_behind_background(&tx, &live).await;
        tx.send(bg_ended("b1")).unwrap();
        tx.send(init(true)).unwrap();
        for _ in 0..100 {
            if !resumed(&app, &key).await.is_empty() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        assert_eq!(
            resumed(&app, &key).await,
            vec![Some("background-done".to_string())]
        );
        assert!(!live.has_held_stop());
        tokio::time::sleep(FALLBACK * 3).await;
        assert_eq!(live.state(), LiveState::Working);
        assert_eq!(status(&app, &key).await, TaskStatus::Running);
        let _ = live.runtime.kill();
    }

    #[tokio::test]
    async fn a_non_primary_session_self_starting_moves_nothing() {
        let app = test_app().await;
        let mut cmd = tokio::process::Command::new("sleep");
        cmd.arg("5");
        let (session, _rx) = delilah_agents::stream::StreamSession::spawn(cmd).unwrap();
        let live = app.sessions.insert(
            "s-1",
            "PROJ-1",
            SessionKind::Claude,
            false,
            std::sync::Arc::new(session),
        );
        let key = running_task(&app).await;
        app.transition(&key, Event::AgentAsked).await.unwrap();
        live.set_state(LiveState::Waiting);
        let (tx, rx) = mpsc::unbounded_channel();
        spawn_pump(&app, "s-1", "PROJ-1", rx);
        tx.send(init(true)).unwrap();
        tx.send(bg_started("b1")).unwrap();
        until(|| live.progress().has_background()).await;
        assert_eq!(live.state(), LiveState::Waiting);
        assert_eq!(status(&app, &key).await, TaskStatus::WaitingForUser);
        assert!(resumed(&app, &key).await.is_empty());
        let _ = live.runtime.kill();
    }

    #[tokio::test]
    async fn a_turn_the_user_started_changes_nothing_here() {
        let app = test_app().await;
        let live = insert_stream(&app, "sleep", &["5"]);
        let key = waiting_task(&app, &live).await;
        let (tx, rx) = mpsc::unbounded_channel();
        spawn_pump(&app, "s-1", "PROJ-1", rx);
        tx.send(init(false)).unwrap();
        // Another event after it: once it is handled, the init has been too.
        tx.send(bg_started("b1")).unwrap();
        until(|| live.progress().has_background()).await;
        assert_eq!(live.state(), LiveState::Waiting);
        assert_eq!(status(&app, &key).await, TaskStatus::WaitingForUser);
        assert!(app.store.attention("s-1").await.unwrap().is_some());
        assert!(resumed(&app, &key).await.is_empty());
        let _ = live.runtime.kill();
    }

    #[tokio::test]
    async fn the_real_prompt_hook_before_or_after_the_init_leaves_the_same_end_state() {
        for hook_first in [true, false] {
            let app = test_app().await;
            let live = insert_stream(&app, "sleep", &["5"]);
            let key = waiting_task(&app, &live).await;
            let hook = json!({ "hook_event_name": "UserPromptSubmit",
                "prompt": "<task-notification>\n<task-id>b1</task-id>" });
            let (tx, rx) = mpsc::unbounded_channel();
            spawn_pump(&app, "s-1", "PROJ-1", rx);
            if hook_first {
                crate::hooks::apply_signal(
                    app.clone(),
                    "PROJ-1",
                    Some("s-1"),
                    delilah_agents::claude::Signal::PromptSubmitted,
                    &hook,
                )
                .await;
            }
            tx.send(init(true)).unwrap();
            tx.send(bg_started("b1")).unwrap();
            until(|| live.progress().has_background()).await;
            if !hook_first {
                crate::hooks::apply_signal(
                    app.clone(),
                    "PROJ-1",
                    Some("s-1"),
                    delilah_agents::claude::Signal::PromptSubmitted,
                    &hook,
                )
                .await;
            }
            assert_eq!(live.state(), LiveState::Working);
            assert_eq!(status(&app, &key).await, TaskStatus::Running);
            assert!(app.store.attention("s-1").await.unwrap().is_none());
            for _ in 0..100 {
                if !resumed(&app, &key).await.is_empty() {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
            assert_eq!(
                resumed(&app, &key).await.len(),
                1,
                "hook_first={hook_first}"
            );
            let _ = live.runtime.kill();
        }
    }
}
