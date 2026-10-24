//! A long-lived `claude -p --input-format stream-json --output-format stream-json` process.
//!
//! The wire shapes (recorded in docs/spikes/2026-10-24-claude-stream.md) stay private to this
//! module: stdout lines become [`StreamEvent`]s on a broadcast channel, and the commands the
//! daemon sends (user turns, interrupts, permission answers) are written to stdin as JSON lines.

use std::io;
use std::process::{ExitStatus, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{ChildStdin, Command};
use tokio::sync::{Mutex, Notify, broadcast, watch};

const EVENT_BUFFER: usize = 1024;

/// One rate-limit window: percent used (0-100) and when it resets (epoch seconds).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LimitWindow {
    pub percent: u32,
    pub resets_at: i64,
}

/// What the process says, in the terms the daemon needs.
#[derive(Debug, Clone, PartialEq)]
pub enum StreamEvent {
    /// The `system` init line.
    Init { session_id: String },
    /// A piece of assistant text; `message` is the id from the preceding `message_start`.
    TextDelta { message: String, text: String },
    /// A complete assistant message (the raw `message` object, for `transcript::parse_message`).
    MessageDone { message: String, value: Value },
    /// A user-role message (tool results, the interrupt marker): the raw `message` object.
    User { value: Value },
    /// The end of a turn. `result` is absent on some errors (e.g. an interrupt).
    TurnResult {
        subtype: String,
        is_error: bool,
        result: Option<String>,
    },
    RateLimit {
        five_hour: Option<LimitWindow>,
        seven_day: Option<LimitWindow>,
    },
    /// `can_use_tool`: the agent waits for [`StreamSession::reply_permission`].
    PermissionRequest {
        request_id: String,
        tool: String,
        input: Value,
        suggestion: Option<Value>,
    },
    /// The process is gone (always the last event).
    Exited { code: Option<i32> },
}

/// The answer to a permission request.
#[derive(Debug, Clone, PartialEq)]
pub enum Decision {
    Allow { input: Value },
    Deny { message: String },
}

pub struct StreamSession {
    stdin: Mutex<Option<ChildStdin>>,
    events: broadcast::Sender<StreamEvent>,
    kill: Arc<Notify>,
    exit: watch::Receiver<Option<Option<i32>>>,
    request_seq: AtomicU64,
}

impl StreamSession {
    /// Starts the prepared command (program, args, env, cwd already set) with piped stdio.
    /// Returns the session and a receiver subscribed before the first line is read.
    pub fn spawn(mut cmd: Command) -> io::Result<(Self, broadcast::Receiver<StreamEvent>)> {
        cmd.stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        let mut child = cmd.spawn()?;
        let stdin = child.stdin.take();
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| io::Error::other("no stdout"))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| io::Error::other("no stderr"))?;

        let (events, rx) = broadcast::channel(EVENT_BUFFER);
        let (exit_tx, exit) = watch::channel(None);
        let kill = Arc::new(Notify::new());

        // Owns the child: waits for it, or kills it on request.
        let kill_signal = kill.clone();
        tokio::spawn(async move {
            let status: io::Result<ExitStatus> = tokio::select! {
                s = child.wait() => s,
                () = kill_signal.notified() => {
                    let _ = child.start_kill();
                    child.wait().await
                }
            };
            let code = status.ok().and_then(|s| s.code());
            let _ = exit_tx.send(Some(code));
        });

        tokio::spawn(async move {
            let mut lines = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                eprintln!("claude (stream) stderr: {line}");
            }
        });

        let tx = events.clone();
        let mut exit_rx = exit.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::new(stdout).lines();
            let mut current = String::new();
            while let Ok(Some(line)) = lines.next_line().await {
                if let Some(event) = parse_line(&line, &mut current) {
                    let _ = tx.send(event);
                }
            }
            let code = match exit_rx.wait_for(Option::is_some).await {
                Ok(v) => (*v).flatten(),
                Err(_) => None,
            };
            let _ = tx.send(StreamEvent::Exited { code });
        });

        let session = Self {
            stdin: Mutex::new(stdin),
            events,
            kill,
            exit,
            request_seq: AtomicU64::new(0),
        };
        Ok((session, rx))
    }

    pub fn subscribe(&self) -> broadcast::Receiver<StreamEvent> {
        self.events.subscribe()
    }

    /// Sends one user turn.
    pub async fn send_user(&self, text: &str) -> io::Result<()> {
        self.write(&json!({"type": "user", "message": {"role": "user", "content": text}}))
            .await
    }

    /// Asks the agent to stop the current turn; the process stays alive for the next turn.
    pub async fn interrupt(&self) -> io::Result<()> {
        let n = self.request_seq.fetch_add(1, Ordering::Relaxed);
        self.write(&json!({
            "type": "control_request",
            "request_id": format!("delilah-interrupt-{n}"),
            "request": {"subtype": "interrupt"},
        }))
        .await
    }

    /// Answers a [`StreamEvent::PermissionRequest`].
    pub async fn reply_permission(&self, request_id: &str, decision: Decision) -> io::Result<()> {
        let response = match decision {
            Decision::Allow { input } => json!({"behavior": "allow", "updatedInput": input}),
            Decision::Deny { message } => json!({"behavior": "deny", "message": message}),
        };
        self.write(&json!({
            "type": "control_response",
            "response": {"subtype": "success", "request_id": request_id, "response": response},
        }))
        .await
    }

    pub fn kill(&self) {
        self.kill.notify_one();
    }

    /// Waits for the process to exit and returns its exit code (None if killed by a signal).
    pub async fn wait(&self) -> Option<i32> {
        let mut exit = self.exit.clone();
        match exit.wait_for(Option::is_some).await {
            Ok(v) => (*v).flatten(),
            Err(_) => None,
        }
    }

    pub fn has_exited(&self) -> bool {
        self.exit.borrow().is_some()
    }

    async fn write(&self, value: &Value) -> io::Result<()> {
        let mut line = value.to_string();
        line.push('\n');
        let mut guard = self.stdin.lock().await;
        let stdin = guard
            .as_mut()
            .ok_or_else(|| io::Error::new(io::ErrorKind::BrokenPipe, "stdin closed"))?;
        stdin.write_all(line.as_bytes()).await?;
        stdin.flush().await
    }
}

fn window(info: &Value, name: &str) -> Option<LimitWindow> {
    let w = info.pointer(&format!("/unifiedWindows/{name}"))?;
    Some(LimitWindow {
        percent: (w.get("utilization")?.as_f64()? * 100.0)
            .round()
            .clamp(0.0, 100.0) as u32,
        resets_at: w.get("resetsAt")?.as_i64()?,
    })
}

/// Maps one stdout line to an event; `current` is the id of the last `message_start`.
/// Unknown or malformed lines give `None`.
fn parse_line(line: &str, current: &mut String) -> Option<StreamEvent> {
    let v: Value = serde_json::from_str(line).ok()?;
    let text = |p: &str| v.pointer(p).and_then(Value::as_str).map(str::to_string);
    match v.get("type")?.as_str()? {
        "system" if v["subtype"] == "init" => Some(StreamEvent::Init {
            session_id: text("/session_id")?,
        }),
        "stream_event" => match v.pointer("/event/type")?.as_str()? {
            "message_start" => {
                *current = text("/event/message/id")?;
                None
            }
            "content_block_delta" if v.pointer("/event/delta/type")? == "text_delta" => {
                Some(StreamEvent::TextDelta {
                    message: current.clone(),
                    text: text("/event/delta/text")?,
                })
            }
            _ => None,
        },
        "assistant" => Some(StreamEvent::MessageDone {
            message: text("/message/id")?,
            value: v.get("message")?.clone(),
        }),
        "user" => Some(StreamEvent::User {
            value: v.get("message")?.clone(),
        }),
        "result" => Some(StreamEvent::TurnResult {
            subtype: text("/subtype")?,
            is_error: v["is_error"].as_bool().unwrap_or(false),
            result: text("/result"),
        }),
        "rate_limit_event" => {
            let info = v.get("rate_limit_info")?;
            Some(StreamEvent::RateLimit {
                five_hour: window(info, "five_hour"),
                seven_day: window(info, "seven_day"),
            })
        }
        "control_request" if v.pointer("/request/subtype")? == "can_use_tool" => {
            Some(StreamEvent::PermissionRequest {
                request_id: text("/request_id")?,
                tool: text("/request/tool_name")?,
                input: v.pointer("/request/input").cloned().unwrap_or(Value::Null),
                suggestion: v.pointer("/request/permission_suggestions").cloned(),
            })
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    fn fake() -> Command {
        let script = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/fake-claude-stream.sh"
        );
        Command::new(script)
    }

    async fn next(rx: &mut broadcast::Receiver<StreamEvent>) -> StreamEvent {
        tokio::time::timeout(Duration::from_secs(10), rx.recv())
            .await
            .expect("timed out waiting for an event")
            .expect("channel closed")
    }

    /// Collects events up to and including the next TurnResult.
    async fn turn(rx: &mut broadcast::Receiver<StreamEvent>) -> Vec<StreamEvent> {
        let mut out = vec![];
        loop {
            let e = next(rx).await;
            let done = matches!(e, StreamEvent::TurnResult { .. });
            out.push(e);
            if done {
                return out;
            }
        }
    }

    #[tokio::test]
    async fn spawn_yields_init() {
        let (s, mut rx) = StreamSession::spawn(fake()).unwrap();
        assert_eq!(
            next(&mut rx).await,
            StreamEvent::Init {
                session_id: "sess-fake".into()
            }
        );
        s.kill();
    }

    #[tokio::test]
    async fn a_turn_streams_deltas_message_limits_and_result() {
        let (s, mut rx) = StreamSession::spawn(fake()).unwrap();
        next(&mut rx).await;
        s.send_user("hi").await.unwrap();
        let events = turn(&mut rx).await;
        let delta = |t: &str| StreamEvent::TextDelta {
            message: "msg_1".into(),
            text: t.into(),
        };
        assert_eq!(events[0], delta("Hel"));
        assert_eq!(events[1], delta("lo"));
        match &events[2] {
            StreamEvent::MessageDone { message, value } => {
                assert_eq!(message, "msg_1");
                assert_eq!(value["content"][0]["text"], "Hello");
            }
            other => panic!("expected MessageDone, got {other:?}"),
        }
        assert_eq!(
            events[3],
            StreamEvent::RateLimit {
                five_hour: Some(LimitWindow {
                    percent: 41,
                    resets_at: 1792872000
                }),
                seven_day: Some(LimitWindow {
                    percent: 29,
                    resets_at: 1793113200
                }),
            }
        );
        assert_eq!(
            events[4],
            StreamEvent::TurnResult {
                subtype: "success".into(),
                is_error: false,
                result: Some("Hello".into())
            }
        );
        s.kill();
    }

    #[tokio::test]
    async fn interrupt_ends_the_turn_and_the_process_takes_another() {
        let (s, mut rx) = StreamSession::spawn(fake()).unwrap();
        next(&mut rx).await;
        s.send_user("WAIT").await.unwrap();
        assert!(matches!(next(&mut rx).await, StreamEvent::TextDelta { .. }));
        s.interrupt().await.unwrap();
        let events = turn(&mut rx).await;
        match &events[0] {
            StreamEvent::User { value } => {
                assert_eq!(value["content"][0]["text"], "[Request interrupted by user]");
            }
            other => panic!("expected User, got {other:?}"),
        }
        assert!(matches!(
            events.last().unwrap(),
            StreamEvent::TurnResult { subtype, is_error: true, result: None } if subtype == "error_during_execution"
        ));
        assert!(!s.has_exited());
        s.send_user("again").await.unwrap();
        let events = turn(&mut rx).await;
        assert!(matches!(
            events.last().unwrap(),
            StreamEvent::TurnResult {
                is_error: false,
                ..
            }
        ));
        s.kill();
    }

    async fn permission_round(decision: Decision) -> Vec<StreamEvent> {
        let (s, mut rx) = StreamSession::spawn(fake()).unwrap();
        next(&mut rx).await;
        s.send_user("NEEDS_PERMISSION").await.unwrap();
        let request = loop {
            if let e @ StreamEvent::PermissionRequest { .. } = next(&mut rx).await {
                break e;
            }
        };
        assert_eq!(
            request,
            StreamEvent::PermissionRequest {
                request_id: "perm-1".into(),
                tool: "Bash".into(),
                input: json!({"command": "npm --version"}),
                suggestion: Some(json!([{"behavior": "allow", "type": "addRules"}])),
            }
        );
        s.reply_permission("perm-1", decision).await.unwrap();
        let events = turn(&mut rx).await;
        s.kill();
        events
    }

    #[tokio::test]
    async fn allow_lets_the_tool_run() {
        let events = permission_round(Decision::Allow {
            input: json!({"command": "npm --version"}),
        })
        .await;
        assert!(matches!(
            events.last().unwrap(),
            StreamEvent::TurnResult { result: Some(r), .. } if r == "allowed"
        ));
    }

    #[tokio::test]
    async fn deny_is_sent_with_its_message() {
        let events = permission_round(Decision::Deny {
            message: "no".into(),
        })
        .await;
        assert!(matches!(
            events.last().unwrap(),
            StreamEvent::TurnResult { result: Some(r), .. } if r == "denied"
        ));
    }

    #[test]
    fn unknown_and_malformed_lines_are_skipped() {
        let mut cur = String::new();
        assert_eq!(parse_line(r#"{"type":"mystery"}"#, &mut cur), None);
        assert_eq!(parse_line("not json", &mut cur), None);
        assert_eq!(
            parse_line(
                r#"{"type":"control_request","request_id":"a","request":{"subtype":"other"}}"#,
                &mut cur
            ),
            None
        );
    }

    #[tokio::test]
    async fn unknown_events_do_not_break_the_turn() {
        let (s, mut rx) = StreamSession::spawn(fake()).unwrap();
        next(&mut rx).await;
        s.send_user("UNKNOWN_EVENT").await.unwrap();
        let events = turn(&mut rx).await;
        assert!(
            events
                .iter()
                .any(|e| matches!(e, StreamEvent::MessageDone { .. }))
        );
        s.kill();
    }

    #[tokio::test]
    async fn kill_and_wait_and_exited() {
        let (s, mut rx) = StreamSession::spawn(fake()).unwrap();
        next(&mut rx).await;
        assert!(!s.has_exited());
        s.kill();
        assert_eq!(s.wait().await, None);
        assert!(s.has_exited());
        assert_eq!(next(&mut rx).await, StreamEvent::Exited { code: None });
    }

    #[tokio::test]
    async fn a_process_that_exits_reports_its_code_and_stderr_is_drained() {
        let (s, mut rx) = StreamSession::spawn(fake()).unwrap();
        next(&mut rx).await;
        s.send_user("STDERR_NOISE EXIT").await.unwrap();
        assert_eq!(s.wait().await, Some(3));
        assert_eq!(next(&mut rx).await, StreamEvent::Exited { code: Some(3) });
    }
}
