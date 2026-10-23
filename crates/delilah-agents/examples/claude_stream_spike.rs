//! Spike for the chat view (specs/006-chat-view, research.md R7): can Claude Code run as
//! a long-lived `claude -p --input-format stream-json --output-format stream-json`
//! process, with hooks and the status line from `--settings`, that Delilah drives with
//! user lines on stdin?
//!
//! Usage (from a plain terminal, not from inside a Claude session; it spends tokens):
//!
//! ```text
//! cargo run -p delilah-agents --example claude_stream_spike -- <scenario> [--out <dir>]
//!                                                              [--prompt-tool stdio|none]
//! ```
//!
//! `<out>` defaults to `$TMPDIR/claude-stream-spike-<scenario>-<unix-seconds>` and is
//! printed at the start and at the end. Nothing is written into the repo. Every scenario
//! runs in a fresh `git init` folder `<out>/workspace` with a clean env
//! (`delilah_agents::env::agent_env()`), ends on its own (timeouts) and leaves in `<out>`:
//!
//! - `claude-version.txt`: `claude --version`.
//! - `settings.json`: the `--settings` file: every hook event (plus PreToolUse `*`)
//!   posts to a local listener; PreToolUse `AskUserQuestion` posts to the blocking ask
//!   endpoint (the listener waits 2 s and answers with the first option, like the daemon);
//!   the status line command appends its stdin to `statusline.jsonl`.
//! - `hooks.jsonl`: `{ts, path, body}` per hook POST (and `{ts, reply}` for the ask hook).
//! - `statusline.jsonl`: `{ts, payload}` per status line run.
//! - `stream.jsonl`: `{ts, proc, dir: "out"|"in", msg}` for every stdout line of the
//!   stream process and every line Delilah wrote on its stdin (control responses too).
//! - `stderr.log`, `pty-<label>.raw`: stderr of the stream process, raw PTY output.
//! - `transcript-<session>.jsonl`: copy of Claude's own transcript at the end, and its
//!   original path in `summary.txt` (also: counts of hooks, stream message types, deltas).
//!
//! Scenarios (R7 letters in brackets):
//!
//! - `turns`: three prompts in one process. [a, b, f, g] Hooks fire? Transcript written
//!   and matching the stream's assistant/user events? Status line runs? Do
//!   `stream_event` `content_block_delta`/`text_delta` carry a message id (see the
//!   `message_start` event before them)?
//! - `resume`: PTY turn, `--resume` stream turn, PTY turn again, same session id. [c]
//!   Does each side see the other's turns (transcript, answers)?
//! - `permission`: asks for `npm --version` with no allow rule, runs with
//!   `--permission-prompt-tool stdio` (override with `--prompt-tool none`); answers the
//!   `control_request` `can_use_tool` first with allow, then with deny. [d2] Does it
//!   arrive with `permission_suggestions`, do both responses work?
//! - `interrupt`: a long answer, a `control_request` `interrupt` 3 s in, then a new prompt.
//!   [e] Does the turn stop, the process stay up, the next line start a turn, which
//!   hooks fire (Stop? StopFailure?)?
//! - `trust`: a brand-new git folder and a brand-new plain folder, one turn each. [d1]
//!   Do hooks from `--settings` fire in an untrusted folder, does the turn run?
//! - `ask`: the model calls AskUserQuestion; the PreToolUse ask hook answers after 2 s. [d3]
//!   Does the hook run and block in print mode, does the model get the answer? Compare
//!   the hook's `ts` in `hooks.jsonl` with the tool result in `stream.jsonl`.
//! - `remote`: `--remote-control <name>` together with `-p`. [h] Does it start, what does
//!   `stderr.log` / `stream.jsonl` say?

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{Receiver, RecvTimeoutError, channel};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use delilah_agents::claude::{self, Launch, Permission};
use portable_pty::{PtySize, native_pty_system};
use serde_json::{Value, json};

const HOOK_EVENTS: &[&str] = &[
    "SessionStart",
    "UserPromptSubmit",
    "PreToolUse",
    "PostToolUse",
    "Notification",
    "Stop",
    "StopFailure",
    "SessionEnd",
];
const TURN_TIMEOUT: Duration = Duration::from_secs(180);
const ASK_PATH: &str = "/hooks/claude/ask";

type Log = Arc<Mutex<std::fs::File>>;

struct Ctx {
    out: PathBuf,
    workspace: PathBuf,
    settings: PathBuf,
    hooks_log: PathBuf,
    stream_log: Log,
    prompt_tool: Option<String>,
    /// (workspace, session id) pairs, to copy their transcripts at the end.
    sessions: Mutex<Vec<(PathBuf, String)>>,
}

fn now() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

fn main() {
    let mut args = std::env::args().skip(1);
    let Some(scenario) = args.next() else { usage() };
    let mut out: Option<PathBuf> = None;
    let mut prompt_tool: Option<String> = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--out" => out = args.next().map(PathBuf::from),
            "--prompt-tool" => prompt_tool = args.next(),
            _ => usage(),
        }
    }
    if ![
        "turns",
        "resume",
        "permission",
        "interrupt",
        "trust",
        "ask",
        "remote",
    ]
    .contains(&scenario.as_str())
    {
        usage();
    }
    // Only `permission` needs the stdio permission prompt by default.
    let prompt_tool = match prompt_tool.as_deref() {
        Some("none") => None,
        Some(other) => Some(other.to_string()),
        None => (scenario == "permission").then(|| "stdio".to_string()),
    };
    let out = out.unwrap_or_else(|| {
        let secs = chrono::Utc::now().timestamp();
        std::env::temp_dir().join(format!("claude-stream-spike-{scenario}-{secs}"))
    });
    println!("out dir: {}", out.display());
    if let Err(e) = run(&scenario, out.clone(), prompt_tool) {
        println!("SPIKE ERROR: {e}");
    }
    println!("\nout dir: {}", out.display());
}

fn usage() -> ! {
    eprintln!(
        "usage: claude_stream_spike <turns|resume|permission|interrupt|trust|ask|remote> \
         [--out <dir>] [--prompt-tool stdio|none]"
    );
    std::process::exit(2);
}

fn run(scenario: &str, out: PathBuf, prompt_tool: Option<String>) -> Result<(), String> {
    let e = |err: std::io::Error| err.to_string();
    std::fs::create_dir_all(&out).map_err(e)?;
    let out = out.canonicalize().map_err(e)?;
    let workspace = out.join("workspace");
    new_git_dir(&workspace)?;

    let version = Command::new("claude")
        .arg("--version")
        .env_clear()
        .envs(delilah_agents::env::agent_env())
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|err| format!("claude --version failed: {err}"));
    println!("claude version: {version}");
    std::fs::write(out.join("claude-version.txt"), format!("{version}\n")).map_err(e)?;

    let hooks_log = out.join("hooks.jsonl");
    let status_log = out.join("statusline.jsonl");
    let listener = TcpListener::bind("127.0.0.1:0").map_err(e)?;
    let port = listener.local_addr().map_err(e)?.port();
    let hooks_file = Arc::new(Mutex::new(std::fs::File::create(&hooks_log).map_err(e)?));
    thread::spawn(move || serve(listener, hooks_file));
    println!("hook listener: 127.0.0.1:{port}");

    let settings = out.join("settings.json");
    std::fs::write(
        &settings,
        serde_json::to_vec_pretty(&settings_json(port, &status_log)).unwrap(),
    )
    .map_err(e)?;

    let stream_log: Log = Arc::new(Mutex::new(
        std::fs::File::create(out.join("stream.jsonl")).map_err(e)?,
    ));
    let ctx = Ctx {
        out: out.clone(),
        workspace,
        settings,
        hooks_log,
        stream_log,
        prompt_tool,
        sessions: Mutex::new(Vec::new()),
    };

    let result = match scenario {
        "turns" => turns(&ctx),
        "resume" => resume(&ctx),
        "permission" => permission(&ctx),
        "interrupt" => interrupt(&ctx),
        "trust" => trust(&ctx),
        "ask" => ask(&ctx),
        "remote" => remote(&ctx),
        _ => unreachable!(),
    };
    if let Err(err) = &result {
        println!("scenario error: {err}");
    }
    thread::sleep(Duration::from_secs(2));
    summarize(&ctx);
    result
}

fn new_git_dir(dir: &Path) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let ok = Command::new("git")
        .args(["init", "-q"])
        .current_dir(dir)
        .status()
        .map_err(|e| e.to_string())?
        .success();
    ok.then_some(())
        .ok_or_else(|| "git init failed".to_string())
}

/// Same shape as `claude::settings`, but posting to the spike's listener.
fn settings_json(port: u16, status_log: &Path) -> Value {
    let base = format!("http://127.0.0.1:{port}");
    let post = |path: &str| {
        format!(
            "curl -sS -m 3 -X POST -H 'content-type: application/json' --data-binary @- \
             '{base}{path}' >/dev/null 2>&1 || true"
        )
    };
    let mut hooks: serde_json::Map<String, Value> = HOOK_EVENTS
        .iter()
        .map(|event| {
            let entry = json!([{ "matcher": "*", "hooks": [{
                "type": "command", "command": post(&format!("/hooks/claude?event={event}"))
            }] }]);
            (event.to_string(), entry)
        })
        .collect();
    let ask = format!(
        "curl -sS -m 86400 -X POST -H 'content-type: application/json' --data-binary @- \
         '{base}{ASK_PATH}?task=SPIKE&session=SPIKE' 2>/dev/null || true"
    );
    hooks.insert(
        "PreToolUse".into(),
        json!([
            { "matcher": "*", "hooks": [{
                "type": "command", "command": post("/hooks/claude?event=PreToolUse")
            }] },
            { "matcher": "AskUserQuestion", "hooks": [{
                "type": "command", "command": ask, "timeout": claude::ASK_TIMEOUT_SECS
            }] },
        ]),
    );
    let status = format!(
        "{{ printf '{{\"ts\":\"%s\",\"payload\":' \"$(date -u +%FT%TZ)\"; cat; echo '}}'; }} \
         >> '{}'; echo delilah",
        status_log.display()
    );
    json!({ "hooks": hooks, "statusLine": { "type": "command", "command": status } })
}

// ---------------------------------------------------------------- hook listener

fn serve(listener: TcpListener, log: Arc<Mutex<std::fs::File>>) {
    for conn in listener.incoming().flatten() {
        let log = Arc::clone(&log);
        thread::spawn(move || {
            let _ = handle(conn, &log);
        });
    }
}

fn append(log: &Arc<Mutex<std::fs::File>>, line: &Value) {
    let mut file = log.lock().unwrap();
    let _ = writeln!(file, "{line}");
}

fn handle(mut conn: TcpStream, log: &Arc<Mutex<std::fs::File>>) -> std::io::Result<()> {
    let mut reader = BufReader::new(conn.try_clone()?);
    let mut request_line = String::new();
    reader.read_line(&mut request_line)?;
    let path = request_line
        .split_whitespace()
        .nth(1)
        .unwrap_or("")
        .to_string();
    let mut length = 0usize;
    loop {
        let mut header = String::new();
        reader.read_line(&mut header)?;
        if header.trim().is_empty() {
            break;
        }
        if let Some((name, value)) = header.split_once(':')
            && name.eq_ignore_ascii_case("content-length")
        {
            length = value.trim().parse().unwrap_or(0);
        }
    }
    let mut body = vec![0u8; length];
    reader.read_exact(&mut body)?;
    let text = String::from_utf8_lossy(&body).to_string();
    let parsed: Value = serde_json::from_str(&text).unwrap_or(Value::String(text));
    append(log, &json!({ "ts": now(), "path": path, "body": parsed }));

    let reply = if path.starts_with(ASK_PATH) {
        // Like the daemon's ask endpoint: hold the hook, then answer with the first option.
        thread::sleep(Duration::from_secs(2));
        let reply = ask_reply(&parsed);
        append(log, &json!({ "ts": now(), "path": path, "reply": reply }));
        reply
    } else {
        json!({})
    };
    let payload = reply.to_string();
    write!(
        conn,
        "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\
         connection: close\r\n\r\n{payload}",
        payload.len()
    )
}

/// The reply of `asks::allow_reply`: allow, with `updatedInput.answers` (question -> label).
fn ask_reply(hook: &Value) -> Value {
    if hook["tool_name"] != "AskUserQuestion" {
        return json!({});
    }
    let questions = hook["tool_input"]["questions"].clone();
    let mut answers = serde_json::Map::new();
    for q in questions.as_array().into_iter().flatten() {
        if let Some(text) = q["question"].as_str() {
            let first = q["options"][0]["label"].as_str().unwrap_or("");
            answers.insert(text.to_string(), json!(first));
        }
    }
    json!({ "hookSpecificOutput": {
        "hookEventName": "PreToolUse",
        "permissionDecision": "allow",
        "updatedInput": { "questions": questions, "answers": answers },
    } })
}

fn hooks(ctx: &Ctx) -> Vec<Value> {
    read_jsonl(&ctx.hooks_log)
        .into_iter()
        .filter_map(|mut l| l.get_mut("body").map(Value::take))
        .collect()
}

fn hook_count(ctx: &Ctx, event: &str) -> usize {
    hooks(ctx)
        .iter()
        .filter(|b| b["hook_event_name"] == event)
        .count()
}

// ------------------------------------------------------------ stream-json process

struct StreamProc {
    label: String,
    child: Child,
    stdin: Option<ChildStdin>,
    rx: Receiver<Value>,
    log: Log,
}

enum Wait {
    Result(Value),
    Timeout,
    Exited,
}

fn log_line(log: &Log, proc: &str, dir: &str, msg: &Value) {
    let mut file = log.lock().unwrap();
    let _ = writeln!(
        file,
        "{}",
        json!({ "ts": now(), "proc": proc, "dir": dir, "msg": msg })
    );
}

impl StreamProc {
    fn spawn(
        ctx: &Ctx,
        label: &str,
        workspace: &Path,
        session: &str,
        resume: bool,
        extra: &[&str],
    ) -> Result<Self, String> {
        let mut cmd = Command::new("claude");
        cmd.args([
            "-p",
            "--input-format",
            "stream-json",
            "--output-format",
            "stream-json",
            "--verbose",
            "--include-partial-messages",
        ]);
        cmd.args([if resume { "--resume" } else { "--session-id" }, session]);
        cmd.arg("--settings").arg(&ctx.settings);
        if let Some(tool) = &ctx.prompt_tool {
            cmd.args(["--permission-prompt-tool", tool]);
        }
        cmd.args(extra);
        cmd.current_dir(workspace)
            .env_clear()
            .envs(delilah_agents::env::agent_env())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::from(
                std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(ctx.out.join("stderr.log"))
                    .map_err(|e| e.to_string())?,
            ));
        println!("[{label}] spawn: {cmd:?}");
        let mut child = cmd
            .spawn()
            .map_err(|e| format!("cannot start claude: {e}"))?;
        let stdin = child.stdin.take();
        let stdout = child.stdout.take().unwrap();
        let (tx, rx) = channel();
        let log = Arc::clone(&ctx.stream_log);
        let (thread_log, thread_label) = (Arc::clone(&log), label.to_string());
        thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                let msg = serde_json::from_str(&line).unwrap_or_else(|_| json!({ "raw": line }));
                log_line(&thread_log, &thread_label, "out", &msg);
                if tx.send(msg).is_err() {
                    break;
                }
            }
        });
        ctx.sessions
            .lock()
            .unwrap()
            .push((workspace.to_path_buf(), session.to_string()));
        Ok(Self {
            label: label.to_string(),
            child,
            stdin,
            rx,
            log,
        })
    }

    fn send(&mut self, msg: &Value) {
        log_line(&self.log, &self.label, "in", msg);
        if let Some(stdin) = self.stdin.as_mut()
            && (writeln!(stdin, "{msg}").is_err() || stdin.flush().is_err())
        {
            println!("[{}] stdin closed", self.label);
        }
    }

    fn send_prompt(&mut self, text: &str) {
        println!("[{}] prompt: {text}", self.label);
        self.send(&json!({ "type": "user", "message": { "role": "user", "content": text } }));
    }

    /// Reads messages until the turn's `result`. `tick` sees every message and, every
    /// 200 ms of silence, `None` (for timers).
    fn run_turn(
        &mut self,
        timeout: Duration,
        tick: &mut dyn FnMut(&mut Self, Option<&Value>),
    ) -> Wait {
        let start = Instant::now();
        while start.elapsed() < timeout {
            match self.rx.recv_timeout(Duration::from_millis(200)) {
                Ok(msg) => {
                    tick(self, Some(&msg));
                    if msg["type"] == "result" {
                        return Wait::Result(msg);
                    }
                }
                Err(RecvTimeoutError::Timeout) => tick(self, None),
                Err(RecvTimeoutError::Disconnected) => return Wait::Exited,
            }
        }
        Wait::Timeout
    }

    /// One prompt and its answer, `tick` as in [`run_turn`](Self::run_turn).
    fn turn(
        &mut self,
        prompt: &str,
        tick: &mut dyn FnMut(&mut Self, Option<&Value>),
    ) -> Result<Value, String> {
        self.send_prompt(prompt);
        self.await_turn(tick)
    }

    fn await_turn(
        &mut self,
        tick: &mut dyn FnMut(&mut Self, Option<&Value>),
    ) -> Result<Value, String> {
        match self.run_turn(TURN_TIMEOUT, tick) {
            Wait::Result(r) => {
                println!(
                    "[{}] result: subtype={} is_error={} text={}",
                    self.label,
                    r["subtype"],
                    r["is_error"],
                    r["result"]
                        .as_str()
                        .unwrap_or("")
                        .chars()
                        .take(120)
                        .collect::<String>()
                );
                Ok(r)
            }
            Wait::Timeout => Err(format!(
                "[{}] no result within {TURN_TIMEOUT:?}",
                self.label
            )),
            Wait::Exited => Err(format!(
                "[{}] the process ended before the result (see stderr.log)",
                self.label
            )),
        }
    }

    /// Closes stdin and waits for the process; kills it after 10 s.
    fn finish(mut self) {
        drop(self.stdin.take());
        let start = Instant::now();
        loop {
            match self.child.try_wait() {
                Ok(Some(status)) => {
                    println!("[{}] exited: {status}", self.label);
                    return;
                }
                Ok(None) if start.elapsed() < Duration::from_secs(10) => {
                    thread::sleep(Duration::from_millis(200))
                }
                _ => {
                    println!("[{}] still running after stdin closed: killing", self.label);
                    let _ = self.child.kill();
                    let _ = self.child.wait();
                    return;
                }
            }
        }
    }
}

/// Logs and answers a `control_request`: `can_use_tool` is allowed or denied, anything
/// else gets an error response.
fn answer_control(p: &mut StreamProc, msg: &Value, allow: bool) {
    let request = &msg["request"];
    let id = msg["request_id"].clone();
    let subtype = request["subtype"].as_str().unwrap_or("?").to_string();
    println!(
        "[{}] control_request {subtype} tool={} suggestions={}",
        p.label,
        request["tool_name"],
        request
            .get("permission_suggestions")
            .unwrap_or(&Value::Null)
    );
    let response = if subtype == "can_use_tool" {
        let body = if allow {
            json!({ "behavior": "allow", "updatedInput": request["input"] })
        } else {
            json!({ "behavior": "deny", "message": "Denied by the spike" })
        };
        json!({ "subtype": "success", "request_id": id, "response": body })
    } else {
        json!({ "subtype": "error", "request_id": id, "error": "unsupported by the spike" })
    };
    p.send(&json!({ "type": "control_response", "response": response }));
}

fn ignore(_: &mut StreamProc, _: Option<&Value>) {}

fn new_session() -> String {
    let id = uuid::Uuid::new_v4().to_string();
    println!("session id: {id}");
    id
}

// ---------------------------------------------------------------------- scenarios

fn turns(ctx: &Ctx) -> Result<(), String> {
    let mut p = StreamProc::spawn(ctx, "stream", &ctx.workspace, &new_session(), false, &[])?;
    let prompts = [
        "Reply with only the word: one",
        "Reply with only the word: two",
        "Which two words did you reply with before? Answer in one short sentence.",
    ];
    let mut result = Ok(());
    for prompt in prompts {
        if let Err(e) = p.turn(prompt, &mut ignore) {
            result = Err(e);
            break;
        }
        thread::sleep(Duration::from_secs(2));
    }
    p.finish();
    result
}

fn resume(ctx: &Ctx) -> Result<(), String> {
    let session = new_session();
    let ws = &ctx.workspace;
    pty_turn(
        ctx,
        "pty-1",
        ws,
        &session,
        false,
        "Reply with only the word: alpha",
        1,
    )?;
    println!("transcript after PTY 1: {}", transcript_lines(ws, &session));

    let mut p = StreamProc::spawn(ctx, "stream", ws, &session, true, &[])?;
    let streamed = p.turn(
        "Which word did you reply with in your first message? Answer with only that word.",
        &mut ignore,
    );
    thread::sleep(Duration::from_secs(2));
    p.finish();
    streamed?;
    println!(
        "transcript after stream: {}",
        transcript_lines(ws, &session)
    );

    pty_turn(
        ctx,
        "pty-2",
        ws,
        &session,
        true,
        "Reply with only the word: omega, then list in a sentence every word you have said so far.",
        2,
    )?;
    println!("transcript after PTY 2: {}", transcript_lines(ws, &session));
    Ok(())
}

fn permission(ctx: &Ctx) -> Result<(), String> {
    let mut p = StreamProc::spawn(ctx, "stream", &ctx.workspace, &new_session(), false, &[])?;
    let mut result = Ok(());
    for (allow, prompt) in [
        (
            true,
            "Run the shell command `npm --version` and tell me its output.",
        ),
        (
            false,
            "Run the shell command `npm --version` again and tell me its output.",
        ),
    ] {
        println!(
            "--- answering can_use_tool with {}",
            if allow { "allow" } else { "deny" }
        );
        let mut tick = |p: &mut StreamProc, msg: Option<&Value>| {
            if let Some(m) = msg
                && m["type"] == "control_request"
            {
                answer_control(p, m, allow);
            }
        };
        if let Err(e) = p.turn(prompt, &mut tick) {
            result = Err(e);
            break;
        }
        thread::sleep(Duration::from_secs(2));
    }
    p.finish();
    result
}

fn interrupt(ctx: &Ctx) -> Result<(), String> {
    let mut p = StreamProc::spawn(ctx, "stream", &ctx.workspace, &new_session(), false, &[])?;
    p.send_prompt(
        "Write a very long essay (at least 3000 words) about the history of the bicycle.",
    );
    let start = Instant::now();
    let mut sent = false;
    let mut tick = |p: &mut StreamProc, _: Option<&Value>| {
        if !sent && start.elapsed() >= Duration::from_secs(3) {
            sent = true;
            println!("[{}] sending interrupt after 3 s", p.label);
            p.send(&json!({
                "type": "control_request",
                "request_id": "x",
                "request": { "subtype": "interrupt" }
            }));
        }
    };
    let first = p.await_turn(&mut tick);
    println!("first turn after interrupt: {first:?}");
    thread::sleep(Duration::from_secs(2));
    let alive = p.child.try_wait().map(|s| s.is_none()).unwrap_or(false);
    println!("process still up after interrupt: {alive}");
    let second = if alive {
        p.turn("Reply with only the word: after-interrupt", &mut ignore)
            .map(|_| ())
    } else {
        Err("the process ended after the interrupt".to_string())
    };
    thread::sleep(Duration::from_secs(2));
    p.finish();
    first.map(|_| ()).and(second)
}

fn trust(ctx: &Ctx) -> Result<(), String> {
    let git_dir = ctx.out.join("fresh-git");
    let plain_dir = ctx.out.join("fresh-plain");
    new_git_dir(&git_dir)?;
    std::fs::create_dir_all(&plain_dir).map_err(|e| e.to_string())?;
    let mut result = Ok(());
    for (label, dir) in [("trust-git", git_dir), ("trust-plain", plain_dir)] {
        let session = new_session();
        println!(
            "--- brand-new folder {} (cwd in hook bodies tells them apart)",
            dir.display()
        );
        let mut p = StreamProc::spawn(ctx, label, &dir, &session, false, &[])?;
        let turn = p.turn(
            "Create a file hello.txt containing 'hi' and tell me when it is done.",
            &mut |p: &mut StreamProc, msg: Option<&Value>| {
                if let Some(m) = msg
                    && m["type"] == "control_request"
                {
                    answer_control(p, m, true);
                }
            },
        );
        println!("hello.txt written: {}", dir.join("hello.txt").exists());
        thread::sleep(Duration::from_secs(2));
        p.finish();
        if let Err(e) = turn {
            result = Err(e);
        }
    }
    result
}

fn ask(ctx: &Ctx) -> Result<(), String> {
    let mut p = StreamProc::spawn(ctx, "stream", &ctx.workspace, &new_session(), false, &[])?;
    let turn = p.turn(
        "Use the AskUserQuestion tool to ask me which colour I prefer, with the options \
         red and blue, then tell me which one I chose.",
        &mut |p: &mut StreamProc, msg: Option<&Value>| {
            if let Some(m) = msg
                && m["type"] == "control_request"
            {
                answer_control(p, m, true);
            }
        },
    );
    println!(
        "ask hook posts: {}",
        hooks(ctx)
            .iter()
            .filter(|b| b["tool_name"] == "AskUserQuestion" && b["hook_event_name"] == "PreToolUse")
            .count()
    );
    thread::sleep(Duration::from_secs(2));
    p.finish();
    turn.map(|_| ())
}

fn remote(ctx: &Ctx) -> Result<(), String> {
    let name = format!("delilah-spike-{}", &new_session()[..8]);
    let mut p = StreamProc::spawn(
        ctx,
        "stream",
        &ctx.workspace,
        &new_session(),
        false,
        &["--remote-control", &name],
    )?;
    let turn = p.turn("Reply with only the word: remote", &mut ignore);
    thread::sleep(Duration::from_secs(3));
    p.finish();
    if let Ok(err) = std::fs::read_to_string(ctx.out.join("stderr.log")) {
        println!("stderr.log:\n{}", err.trim());
    }
    turn.map(|_| ())
}

// ------------------------------------------------------------------------ PTY leg

/// One PTY session (`claude::command`) with `prompt` as its first message; waits until
/// the Stop hook count reaches `stops`, then `/exit`. Accepts a trust dialog if shown.
fn pty_turn(
    ctx: &Ctx,
    label: &str,
    workspace: &Path,
    session: &str,
    resume: bool,
    prompt: &str,
    stops: usize,
) -> Result<(), String> {
    ctx.sessions
        .lock()
        .unwrap()
        .push((workspace.to_path_buf(), session.to_string()));
    let cmd = claude::command(&Launch {
        workspace,
        session_id: session,
        settings: &ctx.settings,
        prompt: Some(prompt),
        resume,
        extra_dirs: &[],
        permission: Permission::Default,
        plugin_dir: None,
        append_system_prompt: None,
        remote_control: None,
        program: None,
    });
    println!("[{label}] PTY prompt: {prompt}");
    let err = |e: &dyn std::fmt::Display| e.to_string();
    let pty = native_pty_system()
        .openpty(PtySize {
            rows: 40,
            cols: 120,
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(|e| err(&e))?;
    let mut child = pty.slave.spawn_command(cmd).map_err(|e| err(&e))?;
    drop(pty.slave);
    let mut reader = pty.master.try_clone_reader().map_err(|e| err(&e))?;
    let mut writer = pty.master.take_writer().map_err(|e| err(&e))?;
    let screen = Arc::new(Mutex::new(Vec::<u8>::new()));
    let sink = Arc::clone(&screen);
    let raw = ctx.out.join(format!("pty-{label}.raw"));
    thread::spawn(move || {
        let mut file = std::fs::File::create(raw).ok();
        let mut buf = [0u8; 4096];
        while let Ok(n) = reader.read(&mut buf) {
            if n == 0 {
                break;
            }
            sink.lock().unwrap().extend_from_slice(&buf[..n]);
            if let Some(f) = file.as_mut() {
                let _ = f.write_all(&buf[..n]);
            }
        }
    });

    let mut trust_sent = false;
    let start = Instant::now();
    let mut done = false;
    while start.elapsed() < TURN_TIMEOUT {
        if !trust_sent
            && String::from_utf8_lossy(&screen.lock().unwrap())
                .to_lowercase()
                .contains("trust")
        {
            println!("[{label}] trust dialog shown: accepting");
            let _ = writer.write_all(b"\r");
            trust_sent = true;
        }
        if hook_count(ctx, "Stop") >= stops {
            done = true;
            break;
        }
        thread::sleep(Duration::from_millis(300));
    }
    thread::sleep(Duration::from_secs(2));
    let _ = writer.write_all(b"/exit\r");
    let exit_start = Instant::now();
    while exit_start.elapsed() < Duration::from_secs(10) {
        if matches!(child.try_wait(), Ok(Some(_))) {
            break;
        }
        thread::sleep(Duration::from_millis(200));
    }
    let _ = child.kill();
    done.then_some(())
        .ok_or_else(|| format!("[{label}] no Stop hook within {TURN_TIMEOUT:?}"))
}

// ------------------------------------------------------------- transcripts, summary

fn transcript_path(workspace: &Path, session: &str) -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    let dir = workspace
        .canonicalize()
        .unwrap_or_else(|_| workspace.to_path_buf());
    let folder: String = dir
        .to_string_lossy()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    Some(
        Path::new(&home)
            .join(".claude/projects")
            .join(folder)
            .join(format!("{session}.jsonl")),
    )
}

fn transcript_lines(workspace: &Path, session: &str) -> String {
    match transcript_path(workspace, session).and_then(|p| std::fs::read_to_string(p).ok()) {
        Some(text) => format!("{} lines", text.lines().count()),
        None => "no transcript file".to_string(),
    }
}

fn read_jsonl(path: &Path) -> Vec<Value> {
    std::fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect()
}

fn bump(map: &mut BTreeMap<String, usize>, key: String) {
    *map.entry(key).or_default() += 1;
}

/// Copies the transcripts into the out dir and writes `summary.txt` (also printed).
fn summarize(ctx: &Ctx) {
    let mut s = String::new();
    let mut sessions = ctx.sessions.lock().unwrap().clone();
    sessions.dedup();
    for (ws, session) in &sessions {
        let line = match transcript_path(ws, session) {
            Some(path) if path.is_file() => {
                let copy = ctx.out.join(format!("transcript-{session}.jsonl"));
                let _ = std::fs::copy(&path, &copy);
                format!(
                    "transcript {session}: {} ({})\n",
                    path.display(),
                    transcript_lines(ws, session)
                )
            }
            Some(path) => format!("transcript {session}: NOT FOUND at {}\n", path.display()),
            None => format!("transcript {session}: HOME not set\n"),
        };
        s.push_str(&line);
    }

    let mut hook_counts = BTreeMap::new();
    for body in hooks(ctx) {
        let event = body["hook_event_name"].as_str().unwrap_or("?");
        let tool = body["tool_name"].as_str().unwrap_or("");
        bump(
            &mut hook_counts,
            format!("{event} {tool}").trim().to_string(),
        );
    }
    s.push_str(&format!("hooks received: {hook_counts:?}\n"));
    s.push_str(&format!(
        "status line runs: {}\n",
        read_jsonl(&ctx.out.join("statusline.jsonl")).len()
    ));

    let mut types = BTreeMap::new();
    let mut deltas_with_id = 0usize;
    let mut deltas = 0usize;
    let mut control_requests = Vec::new();
    for line in read_jsonl(&ctx.out.join("stream.jsonl")) {
        if line["dir"] != "out" {
            continue;
        }
        let msg = &line["msg"];
        let kind = msg["type"].as_str().unwrap_or("?");
        let detail = match kind {
            "stream_event" => format!(
                "stream_event/{}/{}",
                msg["event"]["type"].as_str().unwrap_or("?"),
                msg["event"]["delta"]["type"].as_str().unwrap_or("")
            ),
            "system" => format!("system/{}", msg["subtype"].as_str().unwrap_or("?")),
            other => other.to_string(),
        };
        bump(&mut types, detail.trim_end_matches('/').to_string());
        if msg["event"]["delta"]["type"] == "text_delta" {
            deltas += 1;
            // Where a delta could carry its message id: on the event or the wrapper.
            if msg.get("message_id").is_some()
                || msg["event"].get("message_id").is_some()
                || msg["event"].get("message").is_some()
                || msg.get("uuid").is_some()
            {
                deltas_with_id += 1;
            }
        }
        if kind == "control_request" {
            control_requests.push(msg["request"]["subtype"].to_string());
        }
    }
    s.push_str(&format!("stream message types: {types:?}\n"));
    s.push_str(&format!(
        "text_delta events: {deltas} (with some id field on the wrapper/event: {deltas_with_id}; \
         the wrapper's `uuid` is not a message id: check message_start's message.id in stream.jsonl)\n"
    ));
    s.push_str(&format!(
        "control_requests received: {control_requests:?}\n"
    ));
    let _ = std::fs::write(ctx.out.join("summary.txt"), &s);
    println!("\n== summary ==\n{s}");
}
