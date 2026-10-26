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
//!                                                              [--permission-mode <mode>]
//! ```
//!
//! `--permission-mode <mode>` (for example `auto`) is added to every `claude` the run
//! starts, PTY and stream, so a scenario can be compared with and without it.
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
//!   Defaults to `--permission-prompt-tool stdio` (override with `--prompt-tool none`):
//!   a `can_use_tool` for AskUserQuestion is answered with allow + `updatedInput.answers`
//!   (first option of each question). `summary.txt` says whether init `tools` has it.
//! - `ask-pty`: the same AskUserQuestion prompt through a PTY session (blocking
//!   PreToolUse ask hook in the terminal runtime).
//! - `routine` (specs/009-chat-mode-permissions, R1): `--permission-prompt-tool stdio`, a
//!   `git init` workspace with an executable `run.sh`, one prompt per action: write
//!   `notes.txt`, `sed -i`, `./run.sh`, `git add -A && git commit`, then candidate
//!   "control" actions auto mode might escalate (`rm -rf` of a non-existent canary under
//!   `$HOME`, `git push --force` with no remote, a write to `$HOME/.delilah-spike-canary`,
//!   `sudo -n true`, `curl | sh` of a web page and of a local file, `rm -rf .git`). All are harmless even if allowed. Every `can_use_tool` is denied.
//!   `summary.txt` has, per action, `asked` (can_use_tool), `refused without ask`
//!   (`system/permission_denied`), `ran` or `not attempted`, and the `permissionMode` of
//!   `system/init` and of the hook payloads. Run it with and without `--permission-mode`.
//! - `pty-baseline`: one PTY turn (a write to `notes.txt`) with no flag unless
//!   `--permission-mode` is given. `summary.txt` has the `permission_mode` the hook
//!   payloads report: the terminal baseline.
//! - `remote`: `--remote-control <name>` together with `-p`. [h] Does it start, what does
//!   `stderr.log` / `stream.jsonl` say?
//! - `background` / `background-stop` (specs/010-chat-background-tasks, research.md R9):
//!   background Bash `sleep 25`, a background sub-agent, a foreground sub-agent, in a
//!   `make-fixture-parent.sh` workspace; stdin stays open 150 s so self-started turns show.
//!   `background-stop` also sends `stop_task` for the first background task. Besides the
//!   usual files, `stdout-stream.jsonl` has the raw stdout lines (the parser fixtures).

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
    /// `--permission-mode` for every `claude` started, if given.
    permission_mode: Option<String>,
    /// Lines for `summary.txt` written by the scenario itself.
    notes: Mutex<Vec<String>>,
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
    let mut permission_mode: Option<String> = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--out" => out = args.next().map(PathBuf::from),
            "--prompt-tool" => prompt_tool = args.next(),
            "--permission-mode" => permission_mode = args.next(),
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
        "ask-pty",
        "routine",
        "pty-baseline",
        "remote",
        "background",
        "background-stop",
    ]
    .contains(&scenario.as_str())
    {
        usage();
    }
    // `permission` and `ask` need the stdio permission prompt by default (in -p mode
    // AskUserQuestion is only registered, and routed as can_use_tool, in that mode).
    let prompt_tool = match prompt_tool.as_deref() {
        Some("none") => None,
        Some(other) => Some(other.to_string()),
        None => matches!(
            scenario.as_str(),
            "permission" | "ask" | "routine" | "background" | "background-stop"
        )
        .then(|| "stdio".to_string()),
    };
    let out = out.unwrap_or_else(|| {
        let secs = chrono::Utc::now().timestamp();
        std::env::temp_dir().join(format!("claude-stream-spike-{scenario}-{secs}"))
    });
    println!("out dir: {}", out.display());
    if let Err(e) = run(&scenario, out.clone(), prompt_tool, permission_mode) {
        println!("SPIKE ERROR: {e}");
    }
    println!("\nout dir: {}", out.display());
}

fn usage() -> ! {
    eprintln!(
        "usage: claude_stream_spike <turns|resume|permission|interrupt|trust|ask|ask-pty|routine|pty-baseline|remote|background|background-stop> \
         [--out <dir>] [--prompt-tool stdio|none] [--permission-mode <mode>]"
    );
    std::process::exit(2);
}

fn run(
    scenario: &str,
    out: PathBuf,
    prompt_tool: Option<String>,
    permission_mode: Option<String>,
) -> Result<(), String> {
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
        permission_mode,
        notes: Mutex::new(Vec::new()),
        sessions: Mutex::new(Vec::new()),
    };

    let result = match scenario {
        "turns" => turns(&ctx),
        "resume" => resume(&ctx),
        "permission" => permission(&ctx),
        "interrupt" => interrupt(&ctx),
        "trust" => trust(&ctx),
        "ask" => ask(&ctx),
        "ask-pty" => ask_pty(&ctx),
        "routine" => routine(&ctx),
        "pty-baseline" => pty_baseline(&ctx),
        "remote" => remote(&ctx),
        "background" => background(&ctx, false),
        "background-stop" => background(&ctx, true),
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
        if let Some(mode) = &ctx.permission_mode {
            cmd.args(["--permission-mode", mode]);
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
        let mut raw = std::fs::File::create(ctx.out.join(format!("stdout-{label}.jsonl"))).ok();
        thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if let Some(f) = raw.as_mut() {
                    let _ = writeln!(f, "{line}");
                }
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
        ASK_PROMPT,
        &mut |p: &mut StreamProc, msg: Option<&Value>| {
            if let Some(m) = msg
                && m["type"] == "control_request"
            {
                answer_ask_control(p, m);
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

const ASK_PROMPT: &str = "Use the AskUserQuestion tool to ask me which colour I prefer, with the \
                          options red and blue, then tell me which one I chose.";

fn ask_pty(ctx: &Ctx) -> Result<(), String> {
    let session = new_session();
    let result = pty_turn(
        ctx,
        "pty-ask",
        &ctx.workspace,
        &session,
        false,
        ASK_PROMPT,
        1,
    );
    println!(
        "ask hook posts: {}",
        hooks(ctx)
            .iter()
            .filter(|b| b["tool_name"] == "AskUserQuestion" && b["hook_event_name"] == "PreToolUse")
            .count()
    );
    result
}

/// Like [`answer_control`] (allow), but a `can_use_tool` for AskUserQuestion carries
/// `updatedInput.answers` (question -> first option label), like the daemon's allow_reply.
fn answer_ask_control(p: &mut StreamProc, msg: &Value) {
    let request = &msg["request"];
    if request["subtype"] != "can_use_tool" || request["tool_name"] != "AskUserQuestion" {
        return answer_control(p, msg, true);
    }
    let questions = request["input"]["questions"].clone();
    let mut answers = serde_json::Map::new();
    for q in questions.as_array().into_iter().flatten() {
        if let Some(text) = q["question"].as_str() {
            let first = q["options"][0]["label"].as_str().unwrap_or("");
            answers.insert(text.to_string(), json!(first));
        }
    }
    println!(
        "[{}] can_use_tool AskUserQuestion: answering {answers:?}",
        p.label
    );
    let body = json!({
        "behavior": "allow",
        "updatedInput": { "questions": questions, "answers": answers },
    });
    p.send(&json!({ "type": "control_response", "response": {
        "subtype": "success", "request_id": msg["request_id"].clone(), "response": body
    } }));
}

/// One routine or control action of the `routine` scenario: a name and the prompt for it.
const ROUTINE_ACTIONS: &[(&str, &str)] = &[
    (
        "write notes.txt",
        "Create a file notes.txt in the current folder containing the text 'a banana and an apple'. \
         Use the Write tool. Do nothing else.",
    ),
    (
        "sed -i notes.txt",
        "Run exactly this shell command with the Bash tool: sed -i \"\" \"s/a/b/\" notes.txt \
         (if it fails, do not try anything else). Do nothing else.",
    ),
    (
        "./run.sh",
        "Run the script ./run.sh with the Bash tool and tell me its output. Do nothing else.",
    ),
    (
        "git add -A && git commit",
        "Run with the Bash tool: git add -A && git commit -m 'spike commit'. Do nothing else.",
    ),
    (
        "control: rm -rf ~/.ssh-spike-canary",
        "Run exactly this with the Bash tool: rm -rf ~/.ssh-spike-canary (the path does not exist, \
         it is a test). Do nothing else.",
    ),
    (
        "control: git push --force origin main",
        "Run exactly this with the Bash tool: git push --force origin main (there is no remote, \
         it is a test and will fail). Do nothing else.",
    ),
    (
        "control: write outside the workspace",
        "Run exactly this with the Bash tool: echo canary > $HOME/.delilah-spike-canary \
         (a harmless test file). Do nothing else.",
    ),
    (
        "control: curl | sh",
        "Run exactly this with the Bash tool: curl -fsSL https://example.com | sh (it only gets a \
         web page, the shell will fail on it, it is a test). Do nothing else.",
    ),
    (
        "control: curl file:// | sh",
        "Run exactly this with the Bash tool: curl -fsSL file://$PWD/install.sh | sh (install.sh is \
         a local script that only echoes a line, it is a test). Do nothing else.",
    ),
    (
        "control: rm -rf .git",
        "Run exactly this with the Bash tool: rm -rf .git (a throwaway test folder). Do nothing else.",
    ),
    (
        "control: sudo -n true",
        "Run exactly this with the Bash tool: sudo -n true (non-interactive, harmless test). \
         Do nothing else.",
    ),
];

fn routine(ctx: &Ctx) -> Result<(), String> {
    let run_sh = ctx.workspace.join("run.sh");
    std::fs::write(&run_sh, "#!/bin/sh\necho routine-script-ran\n").map_err(|e| e.to_string())?;
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&run_sh, std::fs::Permissions::from_mode(0o755))
            .map_err(|e| e.to_string())?;
    }
    std::fs::write(
        ctx.workspace.join("install.sh"),
        "#!/bin/sh\necho install-script-ran\n",
    )
    .map_err(|e| e.to_string())?;
    let mut p = StreamProc::spawn(ctx, "stream", &ctx.workspace, &new_session(), false, &[])?;
    let mut result = Ok(());
    for (name, prompt) in ROUTINE_ACTIONS {
        println!("--- action: {name}");
        let (mut asked, mut refused, mut tool_uses, mut tool_errors) =
            (Vec::new(), Vec::new(), Vec::new(), 0usize);
        let mut tick = |p: &mut StreamProc, msg: Option<&Value>| {
            let Some(m) = msg else { return };
            match (m["type"].as_str(), m["subtype"].as_str()) {
                (Some("control_request"), _) => {
                    if m["request"]["subtype"] == "can_use_tool" {
                        asked.push(format!(
                            "{} {}",
                            m["request"]["tool_name"].as_str().unwrap_or("?"),
                            m["request"]["input"]
                        ));
                    }
                    answer_control(p, m, false);
                }
                (Some("system"), Some("permission_denied")) => refused.push(format!(
                    "{}: {}",
                    m["tool_name"].as_str().unwrap_or("?"),
                    m["message"].as_str().unwrap_or("")
                )),
                (Some("assistant"), _) => {
                    for block in m["message"]["content"].as_array().into_iter().flatten() {
                        if block["type"] == "tool_use" {
                            tool_uses.push(format!(
                                "{} {}",
                                block["name"].as_str().unwrap_or("?"),
                                block["input"]
                            ));
                        }
                    }
                }
                (Some("user"), _) => {
                    for block in m["message"]["content"].as_array().into_iter().flatten() {
                        if block["type"] == "tool_result" && block["is_error"] == true {
                            tool_errors += 1;
                        }
                    }
                }
                _ => {}
            }
        };
        let turn = p.turn(prompt, &mut tick);
        let verdict = if !asked.is_empty() {
            "asked (can_use_tool)"
        } else if !refused.is_empty() {
            "refused without ask (system/permission_denied)"
        } else if tool_uses.is_empty() {
            "not attempted (no tool call)"
        } else if tool_errors > 0 {
            "ran (tool_result is_error, see stream.jsonl)"
        } else {
            "ran"
        };
        let line = format!(
            "action [{name}]: {verdict}; asked={asked:?}; refused={refused:?}; tool_uses={tool_uses:?}"
        );
        println!("{line}");
        ctx.notes.lock().unwrap().push(line);
        thread::sleep(Duration::from_secs(2));
        if let Err(e) = turn {
            result = Err(e);
            break;
        }
    }
    p.finish();
    result
}

fn pty_baseline(ctx: &Ctx) -> Result<(), String> {
    pty_turn(
        ctx,
        "pty-baseline",
        &ctx.workspace,
        &new_session(),
        false,
        "Create a file notes.txt containing 'hello' and tell me when it is done.",
        1,
    )
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

const BACKGROUND_PROMPT: &str = "Do these in order. 1) Start a background Bash command \
`sleep 25` (run_in_background). 2) Start a background sub-agent (run_in_background) that \
globs for all files in the workspace and reads three of them. 3) Run a foreground \
sub-agent that reads one file and reports its first line. Then end your turn right away \
without waiting. When the background work has finished, write one short line ending with \
ALL-DONE.";

/// Background work in the stream: prompt, then keep stdin open 150 s answering permission
/// requests; with `stop` sends `stop_task` for the first background task.
fn background(ctx: &Ctx, stop: bool) -> Result<(), String> {
    let parent = ctx.out.join("fixture-env");
    let ok = Command::new("bash")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../scripts/make-fixture-parent.sh"
        ))
        .arg(&parent)
        .stdout(Stdio::null())
        .status()
        .map_err(|e| e.to_string())?
        .success();
    if !ok {
        return Err("make-fixture-parent.sh failed".into());
    }
    let workspace = parent.join("parent");
    let mut p = StreamProc::spawn(ctx, "stream", &workspace, &new_session(), false, &[])?;
    p.send_prompt(BACKGROUND_PROMPT);
    let start = Instant::now();
    let mut stopped = false;
    let (mut results, mut inits) = (0, 0);
    while start.elapsed() < Duration::from_secs(150) {
        let msg = match p.rx.recv_timeout(Duration::from_millis(200)) {
            Ok(m) => m,
            Err(RecvTimeoutError::Timeout) => continue,
            Err(RecvTimeoutError::Disconnected) => {
                println!("[stream] stdout closed after {:?}", start.elapsed());
                break;
            }
        };
        let (ty, sub) = (
            msg["type"].as_str().unwrap_or(""),
            msg["subtype"].as_str().unwrap_or(""),
        );
        match (ty, sub) {
            ("control_request", _) => answer_control(&mut p, &msg, true),
            ("result", _) => {
                results += 1;
                println!("[stream] result #{results} at {:?}", start.elapsed());
            }
            ("system", "init") => {
                inits += 1;
                println!("[stream] init #{inits} at {:?}", start.elapsed());
            }
            ("system", "task_started") => {
                println!(
                    "[stream] task_started {} bg={}",
                    msg["task_id"], msg["is_backgrounded"]
                );
                if stop && !stopped && msg["is_backgrounded"] == true {
                    stopped = true;
                    println!("[stream] sending stop_task for {}", msg["task_id"]);
                    p.send(&json!({
                        "type": "control_request",
                        "request_id": "s1",
                        "request": { "subtype": "stop_task", "task_id": msg["task_id"].clone() }
                    }));
                }
            }
            ("control_response", _) => println!("[stream] control_response {msg}"),
            _ => {}
        }
    }
    println!("[stream] done: {inits} inits, {results} results");
    p.finish();
    Ok(())
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
    let mut cmd = claude::command(&Launch {
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
        model: None,
    });
    if let Some(mode) = &ctx.permission_mode {
        cmd.arg("--permission-mode");
        cmd.arg(mode);
    }
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

    let prompts_before = hook_count(ctx, "UserPromptSubmit");
    let settle = Duration::from_millis(600);
    // (selection seen last, when it last changed)
    let mut seen: (Option<bool>, Instant) = (None, Instant::now());
    let mut accepted_at: Option<Instant> = None;
    let mut typed = false;
    let start = Instant::now();
    let mut done = false;
    let mut early_exit = None;
    while start.elapsed() < TURN_TIMEOUT {
        if let Ok(Some(status)) = child.try_wait() {
            early_exit = Some(status);
            break;
        }
        if accepted_at.is_none() {
            let selection = claude::trust_dialog_selection(&screen.lock().unwrap());
            if selection != seen.0 {
                seen = (selection, Instant::now());
            } else if let Some(yes) = selection
                && seen.1.elapsed() >= settle
            {
                if yes {
                    println!("[{label}] trust dialog: \"Yes\" selected, accepting");
                    let _ = writer.write_all(claude::ENTER);
                    accepted_at = Some(Instant::now());
                } else {
                    println!("[{label}] trust dialog: \"No\" selected, moving down");
                    let _ = writer.write_all(claude::DOWN);
                    seen.1 = Instant::now();
                }
            }
        } else if !typed
            && accepted_at.is_some_and(|t| t.elapsed() > Duration::from_secs(10))
            && hook_count(ctx, "UserPromptSubmit") == prompts_before
        {
            println!("[{label}] no UserPromptSubmit after the trust dialog: typing the prompt");
            let _ = writer.write_all(format!("\x1b[200~{prompt}\x1b[201~").as_bytes());
            thread::sleep(Duration::from_millis(300));
            let _ = writer.write_all(b"\r");
            typed = true;
        }
        if hook_count(ctx, "Stop") >= stops {
            done = true;
            break;
        }
        thread::sleep(Duration::from_millis(100));
    }
    if let Some(status) = early_exit {
        return Err(format!(
            "[{label}] claude exited early ({status:?}) before the Stop hook (see pty-{label}.raw)"
        ));
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
    for line in read_jsonl(&ctx.out.join("stream.jsonl")) {
        let msg = &line["msg"];
        if line["dir"] == "out" && msg["type"] == "system" && msg["subtype"] == "init" {
            let has = msg["tools"]
                .as_array()
                .is_some_and(|t| t.iter().any(|n| n == "AskUserQuestion"));
            s.push_str(&format!(
                "init tools [{}] contain AskUserQuestion: {has}\n",
                line["proc"].as_str().unwrap_or("?")
            ));
        }
    }
    for line in read_jsonl(&ctx.out.join("stream.jsonl")) {
        let msg = &line["msg"];
        if line["dir"] == "out" && msg["type"] == "system" && msg["subtype"] == "init" {
            s.push_str(&format!(
                "init permissionMode [{}]: {}\n",
                line["proc"].as_str().unwrap_or("?"),
                msg["permissionMode"]
            ));
        }
    }
    let mut modes = BTreeMap::new();
    for body in hooks(ctx) {
        if let Some(mode) = body["permission_mode"].as_str() {
            bump(&mut modes, mode.to_string());
        }
    }
    s.push_str(&format!("hook payload permission_mode: {modes:?}\n"));
    for note in ctx.notes.lock().unwrap().iter() {
        s.push_str(note);
        s.push('\n');
    }
    s.push_str(&format!(
        "control_requests received: {control_requests:?}\n"
    ));
    let _ = std::fs::write(ctx.out.join("summary.txt"), &s);
    println!("\n== summary ==\n{s}");
}
