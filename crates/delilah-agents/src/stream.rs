//! A long-lived `claude -p --input-format stream-json --output-format stream-json` process.
//!
//! The wire shapes (recorded in docs/spikes/2026-10-24-claude-stream.md) stay private to this
//! module: stdout lines become [`StreamEvent`]s on a broadcast channel (live text, which may
//! lag) and, but for the text deltas, on a lossless one ([`StreamSession::take_events`], what
//! the daemon's state follows). The commands the daemon sends (user turns, interrupts,
//! permission answers) are written to stdin as JSON lines.
//!
//! Like the real CLI, the process says nothing but hook lines until it has read the first
//! user turn: its `system/init` comes once per turn, after the turn's user line.

use std::io;
use std::path::PathBuf;
use std::process::{ExitStatus, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::{Value, json};
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{ChildStdin, Command};
use tokio::sync::{Mutex, Notify, broadcast, mpsc, watch};

const EVENT_BUFFER: usize = 1024;
/// How many of the process's last stderr lines are kept, for an error message.
const STDERR_TAIL: usize = 20;
/// The longest stdout line read (a message with a big tool result); a longer one is dropped.
const STDOUT_LINE_MAX: usize = 16 * 1024 * 1024;
/// The longest stderr line kept; a longer one is dropped.
const STDERR_LINE_MAX: usize = 64 * 1024;

/// One line read with a cap.
#[derive(Debug, PartialEq, Eq)]
enum Line {
    Text(String),
    /// Longer than the cap: skipped up to its newline.
    TooLong,
}

/// Reads the next line, never holding more than `cap` bytes of it. `None` at the end.
async fn read_line_capped<R: AsyncBufRead + Unpin>(
    reader: &mut R,
    cap: usize,
) -> io::Result<Option<Line>> {
    let mut buf: Vec<u8> = Vec::new();
    let mut too_long = false;
    let mut any = false;
    loop {
        let chunk = reader.fill_buf().await?;
        if chunk.is_empty() {
            return Ok(any.then(|| finish(buf, too_long)));
        }
        any = true;
        let (take, found) = match chunk.iter().position(|&b| b == b'\n') {
            Some(i) => (i, true),
            None => (chunk.len(), false),
        };
        if !too_long {
            if buf.len() + take > cap {
                too_long = true;
                buf = Vec::new();
            } else {
                buf.extend_from_slice(&chunk[..take]);
            }
        }
        reader.consume(take + usize::from(found));
        if found {
            return Ok(Some(finish(buf, too_long)));
        }
    }
}

fn finish(mut buf: Vec<u8>, too_long: bool) -> Line {
    if too_long {
        return Line::TooLong;
    }
    if buf.last() == Some(&b'\r') {
        buf.pop();
    }
    Line::Text(String::from_utf8_lossy(&buf).into_owned())
}

/// One rate-limit window: percent used (0-100) and when it resets (epoch seconds).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LimitWindow {
    pub percent: u32,
    pub resets_at: i64,
}

/// What the process says, in the terms the daemon needs.
#[derive(Debug, Clone, PartialEq)]
pub enum StreamEvent {
    /// The `system` init line (once per turn, after the user line that starts it).
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
        /// The limit is hit: a status other than `allowed`/`allowed_warning`, or a full window.
        reached: bool,
        /// Which window the event is about (`rateLimitType`), and when it resets.
        kind: Option<String>,
        resets_at: Option<i64>,
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
    stderr_tail: Arc<std::sync::Mutex<std::collections::VecDeque<String>>>,
    /// The lossless events, until the pump takes them.
    state_events: std::sync::Mutex<Option<mpsc::UnboundedReceiver<StreamEvent>>>,
    /// How many user turns were sent, and how many there were when an interrupt was last
    /// asked (`NO_INTERRUPT` for none): tells a late interrupt result from a new turn's.
    prompts: AtomicU64,
    interrupted_at: AtomicU64,
}

const NO_INTERRUPT: u64 = u64::MAX;

impl StreamSession {
    /// Starts the prepared command (program, args, env, cwd already set) with piped stdio.
    /// Returns the session and a receiver subscribed before the first line is read.
    pub fn spawn(cmd: Command) -> io::Result<(Self, broadcast::Receiver<StreamEvent>)> {
        Self::spawn_logged(cmd, None)
    }

    /// As [`spawn`](Self::spawn), also appending the process's stderr to the `log` file.
    pub fn spawn_logged(
        mut cmd: Command,
        log: Option<PathBuf>,
    ) -> io::Result<(Self, broadcast::Receiver<StreamEvent>)> {
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
        let (state_tx, state_rx) = mpsc::unbounded_channel();
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

        let stderr_tail = Arc::new(std::sync::Mutex::new(std::collections::VecDeque::new()));
        let tail = stderr_tail.clone();
        let mut log = log.and_then(|path| {
            std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
                .ok()
        });
        let stderr_done = tokio::spawn(async move {
            let mut reader = BufReader::new(stderr);
            while let Ok(Some(line)) = read_line_capped(&mut reader, STDERR_LINE_MAX).await {
                let Line::Text(line) = line else { continue };
                // A short blocking write: one line, to a local file.
                if let Some(file) = &mut log
                    && std::io::Write::write_all(file, format!("{line}\n").as_bytes()).is_err()
                {
                    log = None;
                }
                let mut tail = tail.lock().unwrap();
                if tail.len() == STDERR_TAIL {
                    tail.pop_front();
                }
                tail.push_back(line);
            }
        });

        let tx = events.clone();
        let mut exit_rx = exit.clone();
        tokio::spawn(async move {
            let mut reader = BufReader::new(stdout);
            let mut parser = Parser::default();
            while let Ok(Some(line)) = read_line_capped(&mut reader, STDOUT_LINE_MAX).await {
                let Line::Text(line) = line else { continue };
                for event in parser.parse(&line) {
                    publish(&tx, &state_tx, event);
                }
            }
            for event in parser.flush() {
                publish(&tx, &state_tx, event);
            }
            let code = match exit_rx.wait_for(Option::is_some).await {
                Ok(v) => (*v).flatten(),
                Err(_) => None,
            };
            // So the stderr tail is complete by the time anyone hears of the exit.
            let _ = stderr_done.await;
            publish(&tx, &state_tx, StreamEvent::Exited { code });
        });

        let session = Self {
            stdin: Mutex::new(stdin),
            events,
            kill,
            exit,
            request_seq: AtomicU64::new(0),
            stderr_tail,
            state_events: std::sync::Mutex::new(Some(state_rx)),
            prompts: AtomicU64::new(0),
            interrupted_at: AtomicU64::new(NO_INTERRUPT),
        };
        Ok((session, rx))
    }

    /// The last lines the process wrote to stderr, oldest first, joined by newlines.
    pub fn stderr_tail(&self) -> String {
        let tail = self.stderr_tail.lock().unwrap();
        tail.iter()
            .map(String::as_str)
            .collect::<Vec<_>>()
            .join("\n")
    }

    pub fn subscribe(&self) -> broadcast::Receiver<StreamEvent> {
        self.events.subscribe()
    }

    /// Every event but the text deltas, in order and never dropped (the end is `Exited`),
    /// for whoever keeps state from them. Given once; unread events wait for the taker.
    pub fn take_events(&self) -> Option<mpsc::UnboundedReceiver<StreamEvent>> {
        self.state_events.lock().unwrap().take()
    }

    /// Whether the interrupt asked last is still the latest word: false when a user turn
    /// was sent since (a late `error_during_execution` then is not that turn's). Without
    /// an interrupt asked there is nothing to contradict, so true. Clears the request.
    pub fn take_interrupt_current(&self) -> bool {
        let at = self.interrupted_at.swap(NO_INTERRUPT, Ordering::SeqCst);
        at == NO_INTERRUPT || self.prompts.load(Ordering::SeqCst) <= at
    }

    /// Sends one user turn.
    pub async fn send_user(&self, text: &str) -> io::Result<()> {
        self.prompts.fetch_add(1, Ordering::SeqCst);
        self.write(&json!({"type": "user", "message": {"role": "user", "content": text}}))
            .await
    }

    /// Asks the agent to stop the current turn; the process stays alive for the next turn.
    pub async fn interrupt(&self) -> io::Result<()> {
        self.interrupted_at
            .store(self.prompts.load(Ordering::SeqCst), Ordering::SeqCst);
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

/// Sends an event to the live channel (which may lag) and, but for the text, to the lossless one.
fn publish(
    live: &broadcast::Sender<StreamEvent>,
    state: &mpsc::UnboundedSender<StreamEvent>,
    event: StreamEvent,
) {
    if !matches!(event, StreamEvent::TextDelta { .. }) {
        let _ = state.send(event.clone());
    }
    let _ = live.send(event);
}

/// Reads stdout lines into events, keeping what spans lines: the id of the message being
/// written and its `assistant` lines, which become one `MessageDone` when the message ends.
#[derive(Default)]
struct Parser {
    /// The id of the last `message_start`.
    current: String,
    /// The assistant message so far (one `assistant` line per content block).
    pending: Option<(String, Value)>,
    /// A `message_stop` was seen for this id: its later `assistant` lines stand alone.
    stopped: Option<String>,
}

impl Parser {
    fn flush(&mut self) -> Vec<StreamEvent> {
        match self.pending.take() {
            Some((message, value)) => vec![StreamEvent::MessageDone { message, value }],
            None => vec![],
        }
    }

    /// Maps one stdout line to events. Unknown or malformed lines give none.
    fn parse(&mut self, line: &str) -> Vec<StreamEvent> {
        let Ok(v) = serde_json::from_str::<Value>(line) else {
            return vec![];
        };
        let text = |p: &str| v.pointer(p).and_then(Value::as_str).map(str::to_string);
        let Some(kind) = v.get("type").and_then(Value::as_str) else {
            return vec![];
        };
        match kind {
            "system" if v["subtype"] == "init" => text("/session_id")
                .map(|session_id| vec![StreamEvent::Init { session_id }])
                .unwrap_or_default(),
            "stream_event" => match v.pointer("/event/type").and_then(Value::as_str) {
                Some("message_start") => {
                    let out = self.flush();
                    if let Some(id) = text("/event/message/id") {
                        self.current = id;
                    }
                    self.stopped = None;
                    out
                }
                Some("message_stop") => {
                    let out = self.flush();
                    self.stopped = Some(self.current.clone());
                    out
                }
                Some("content_block_delta")
                    if v.pointer("/event/delta/type") == Some(&json!("text_delta")) =>
                {
                    text("/event/delta/text")
                        .map(|text| {
                            vec![StreamEvent::TextDelta {
                                message: self.current.clone(),
                                text,
                            }]
                        })
                        .unwrap_or_default()
                }
                _ => vec![],
            },
            "assistant" => {
                let (Some(id), Some(message)) = (text("/message/id"), v.get("message")) else {
                    return vec![];
                };
                if self.stopped.as_deref() == Some(id.as_str()) {
                    return vec![StreamEvent::MessageDone {
                        message: id,
                        value: message.clone(),
                    }];
                }
                let mut out = vec![];
                match &mut self.pending {
                    Some((pending, value)) if *pending == id => {
                        // The next content block of the same message.
                        if let (Some(have), Some(more)) = (
                            value.get_mut("content").and_then(Value::as_array_mut),
                            message.get("content").and_then(Value::as_array),
                        ) {
                            have.extend(more.iter().cloned());
                        }
                    }
                    _ => {
                        out = self.flush();
                        self.pending = Some((id, message.clone()));
                    }
                }
                out
            }
            "user" => {
                let mut out = self.flush();
                if let Some(value) = v.get("message") {
                    out.push(StreamEvent::User {
                        value: value.clone(),
                    });
                }
                out
            }
            "result" => {
                let mut out = self.flush();
                if let Some(subtype) = text("/subtype") {
                    out.push(StreamEvent::TurnResult {
                        subtype,
                        is_error: v["is_error"].as_bool().unwrap_or(false),
                        result: text("/result"),
                    });
                }
                out
            }
            "rate_limit_event" => {
                let Some(info) = v.get("rate_limit_info") else {
                    return vec![];
                };
                let five_hour = window(info, "five_hour");
                let seven_day = window(info, "seven_day");
                let full = |w: &Option<LimitWindow>| w.is_some_and(|w| w.percent >= 100);
                // Only a status that says the limit blocks counts; one this adapter does
                // not know is taken as benign rather than pausing the task on a guess.
                let reached = info.get("status").and_then(Value::as_str).is_some_and(|s| {
                    let s = s.to_lowercase();
                    s.contains("reject") || s.contains("exceeded")
                }) || full(&five_hour)
                    || full(&seven_day);
                vec![StreamEvent::RateLimit {
                    five_hour,
                    seven_day,
                    reached,
                    kind: info
                        .get("rateLimitType")
                        .and_then(Value::as_str)
                        .map(str::to_string),
                    resets_at: info.get("resetsAt").and_then(Value::as_i64),
                }]
            }
            "control_request" if v.pointer("/request/subtype") == Some(&json!("can_use_tool")) => {
                match (text("/request_id"), text("/request/tool_name")) {
                    (Some(request_id), Some(tool)) => vec![StreamEvent::PermissionRequest {
                        request_id,
                        tool,
                        input: v.pointer("/request/input").cloned().unwrap_or(Value::Null),
                        suggestion: v.pointer("/request/permission_suggestions").cloned(),
                    }],
                    _ => vec![],
                }
            }
            _ => vec![],
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    #[tokio::test]
    async fn an_overlong_line_is_dropped_and_reading_resyncs_at_the_next_newline() {
        let mut data = b"short\n".to_vec();
        data.extend(std::iter::repeat_n(b'x', 100));
        data.extend_from_slice(b"\nnext\nlast");
        let mut reader = BufReader::with_capacity(8, &data[..]);
        let mut got = Vec::new();
        while let Some(line) = read_line_capped(&mut reader, 10).await.unwrap() {
            got.push(line);
        }
        assert_eq!(
            got,
            [
                Line::Text("short".into()),
                Line::TooLong,
                Line::Text("next".into()),
                Line::Text("last".into()),
            ]
        );
    }

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
    async fn init_comes_after_the_first_user_line_not_at_spawn() {
        let (s, mut rx) = StreamSession::spawn(fake()).unwrap();
        // Like the real CLI: nothing the daemon cares about before the first turn.
        assert!(
            tokio::time::timeout(Duration::from_millis(400), rx.recv())
                .await
                .is_err()
        );
        s.send_user("hi").await.unwrap();
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
        s.send_user("hi").await.unwrap();
        let mut events = turn(&mut rx).await;
        assert!(matches!(events.remove(0), StreamEvent::Init { .. }));
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
                reached: false,
                kind: None,
                resets_at: None,
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
        s.send_user("WAIT").await.unwrap();
        loop {
            if matches!(next(&mut rx).await, StreamEvent::TextDelta { .. }) {
                break;
            }
        }
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
        let mut p = Parser::default();
        assert!(p.parse(r#"{"type":"mystery"}"#).is_empty());
        assert!(p.parse("not json").is_empty());
        assert!(
            p.parse(r#"{"type":"control_request","request_id":"a","request":{"subtype":"other"}}"#)
                .is_empty()
        );
    }

    #[tokio::test]
    async fn unknown_events_do_not_break_the_turn() {
        let (s, mut rx) = StreamSession::spawn(fake()).unwrap();
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
        assert!(!s.has_exited());
        s.kill();
        assert_eq!(s.wait().await, None);
        assert!(s.has_exited());
        assert_eq!(next(&mut rx).await, StreamEvent::Exited { code: None });
    }

    #[tokio::test]
    async fn a_process_that_exits_reports_its_code_and_stderr_is_drained() {
        let (s, mut rx) = StreamSession::spawn(fake()).unwrap();
        s.send_user("STDERR_NOISE EXIT").await.unwrap();
        assert_eq!(s.wait().await, Some(3));
        assert_eq!(next(&mut rx).await, StreamEvent::Exited { code: Some(3) });
    }

    #[tokio::test]
    async fn the_stderr_tail_is_complete_when_the_exit_is_reported() {
        let mut cmd = Command::new("sh");
        cmd.args(["-c", "echo one >&2; echo two >&2; exit 2"]);
        let (s, mut rx) = StreamSession::spawn(cmd).unwrap();
        assert_eq!(next(&mut rx).await, StreamEvent::Exited { code: Some(2) });
        assert_eq!(s.stderr_tail(), "one\ntwo");
    }

    #[test]
    fn a_message_is_done_at_message_stop_with_all_its_blocks() {
        let mut p = Parser::default();
        let lines = [
            r#"{"type":"stream_event","event":{"type":"message_start","message":{"id":"m1"}}}"#,
            r#"{"type":"assistant","message":{"id":"m1","content":[{"type":"text","text":"a"}]}}"#,
            r#"{"type":"stream_event","event":{"type":"content_block_delta","delta":{"type":"text_delta","text":"b"}}}"#,
            r#"{"type":"assistant","message":{"id":"m1","content":[{"type":"tool_use","id":"t"}]}}"#,
        ];
        for line in lines {
            let events = p.parse(line);
            assert!(
                events
                    .iter()
                    .all(|e| !matches!(e, StreamEvent::MessageDone { .. })),
                "no done before the message ends: {events:?}"
            );
        }
        let done = p.parse(r#"{"type":"stream_event","event":{"type":"message_stop"}}"#);
        match &done[..] {
            [StreamEvent::MessageDone { message, value }] => {
                assert_eq!(message, "m1");
                assert_eq!(value["content"].as_array().unwrap().len(), 2);
            }
            other => panic!("expected one MessageDone, got {other:?}"),
        }
    }

    #[test]
    fn a_rate_limit_event_says_whether_the_limit_is_reached() {
        let reached = |info: &str| {
            let line = format!(r#"{{"type":"rate_limit_event","rate_limit_info":{info}}}"#);
            match Parser::default().parse(&line).remove(0) {
                StreamEvent::RateLimit { reached, .. } => reached,
                other => panic!("{other:?}"),
            }
        };
        assert!(!reached(r#"{"status":"allowed"}"#));
        assert!(!reached(r#"{"status":"allowed_warning"}"#));
        assert!(reached(
            r#"{"status":"rejected","rateLimitType":"five_hour"}"#
        ));
        assert!(reached(r#"{"status":"limit_exceeded"}"#));
        assert!(reached(r#"{"status":"Rejected_Hard"}"#));
        // A status this adapter does not know is not a reason to pause.
        assert!(!reached(r#"{"status":"queued"}"#));
        assert!(!reached(r#"{"status":"throttled_softly"}"#));
        assert!(reached(
            r#"{"status":"allowed","unifiedWindows":{"seven_day":{"utilization":1.0,"resetsAt":5}}}"#
        ));
    }

    #[tokio::test]
    async fn the_lossless_channel_keeps_state_events_when_the_live_one_lags() {
        let (s, mut live) = StreamSession::spawn({
            let mut cmd = Command::new("sh");
            cmd.args([
                "-c",
                "i=0; while [ $i -lt 3000 ]; do echo '{\"type\":\"stream_event\",\"event\":{\"type\":\"content_block_delta\",\"delta\":{\"type\":\"text_delta\",\"text\":\"x\"}}}'; i=$((i+1)); done; echo '{\"type\":\"result\",\"subtype\":\"success\",\"is_error\":false,\"result\":\"ok\"}'",
            ]);
            cmd
        })
        .unwrap();
        let mut state = s.take_events().unwrap();
        assert!(s.take_events().is_none());
        let mut seen = vec![];
        while let Some(e) = tokio::time::timeout(Duration::from_secs(10), state.recv())
            .await
            .unwrap()
        {
            seen.push(e);
        }
        assert!(
            matches!(seen[0], StreamEvent::TurnResult { .. }),
            "{seen:?}"
        );
        assert!(matches!(seen.last(), Some(StreamEvent::Exited { .. })));
        assert_eq!(seen.len(), 2, "no text deltas on the lossless channel");
        // The live channel did lag: the text is allowed to be lost.
        assert!(matches!(
            live.recv().await,
            Err(broadcast::error::RecvError::Lagged(_))
        ));
    }

    #[tokio::test]
    async fn a_late_interrupt_result_is_told_from_a_new_turns() {
        let (s, _rx) = StreamSession::spawn(fake()).unwrap();
        // No interrupt asked: a result error stands.
        assert!(s.take_interrupt_current());
        s.interrupt().await.unwrap();
        assert!(s.take_interrupt_current());
        // Asked, then a new turn sent: the late result is not about that turn.
        s.interrupt().await.unwrap();
        s.send_user("again").await.unwrap();
        assert!(!s.take_interrupt_current());
        s.kill();
    }

    #[tokio::test]
    async fn stderr_goes_to_the_log_file() {
        let log = std::env::temp_dir().join(format!("delilah-stream-{}.log", uuid::Uuid::new_v4()));
        let mut cmd = Command::new("sh");
        cmd.args(["-c", "echo one >&2; echo two >&2"]);
        let (_s, mut rx) = StreamSession::spawn_logged(cmd, Some(log.clone())).unwrap();
        assert_eq!(next(&mut rx).await, StreamEvent::Exited { code: Some(0) });
        assert_eq!(std::fs::read_to_string(&log).unwrap(), "one\ntwo\n");
        let _ = std::fs::remove_file(&log);
    }
}
