//! The hooks of a fake session: read from the settings file the daemon passed with
//! `--settings`, run with `sh -c` the way Claude Code does, so the daemon can't tell
//! the fake from the real thing.

use std::io::{Read, Write};
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use serde_json::{Value, json};

/// Timeout (seconds) when a hook declares none.
const DEFAULT_TIMEOUT: u64 = 60;

struct Entry {
    event: String,
    matcher: String,
    command: String,
    timeout: u64,
}

/// The hooks of one settings file.
pub struct Hooks {
    entries: Vec<Entry>,
}

/// What running one event produced. `exit_code` is -1 when the hook was killed
/// (timeout) or could not be started.
#[derive(Debug, Clone, PartialEq)]
pub struct HookRun {
    pub exit_code: i32,
    pub stdout: String,
}

impl Hooks {
    /// Reads `settings.hooks` from the file at `path`.
    pub fn load(path: &Path) -> Result<Hooks, String> {
        let text = std::fs::read_to_string(path)
            .map_err(|e| format!("cannot read settings {}: {e}", path.display()))?;
        let value: Value = serde_json::from_str(&text)
            .map_err(|e| format!("invalid settings {}: {e}", path.display()))?;
        Ok(Hooks::from_value(&value))
    }

    fn from_value(settings: &Value) -> Hooks {
        let mut entries = Vec::new();
        if let Some(events) = settings.get("hooks").and_then(Value::as_object) {
            for (event, groups) in events {
                for group in groups.as_array().into_iter().flatten() {
                    let matcher = group["matcher"].as_str().unwrap_or("*").to_string();
                    for hook in group["hooks"].as_array().into_iter().flatten() {
                        let Some(command) = hook["command"].as_str() else {
                            continue;
                        };
                        entries.push(Entry {
                            event: event.clone(),
                            matcher: matcher.clone(),
                            command: command.to_string(),
                            timeout: hook["timeout"].as_u64().unwrap_or(DEFAULT_TIMEOUT),
                        });
                    }
                }
            }
        }
        Hooks { entries }
    }

    /// The task key, taken from the `task` query parameter of a hook URL.
    pub fn task(&self) -> Option<String> {
        self.entries
            .iter()
            .find_map(|e| query_param(&e.command, "task"))
    }

    /// Runs every command of `event` whose matcher applies (`*`, or
    /// `AskUserQuestion` for `PreToolUse`) with `payload` on stdin. Returns one run
    /// per command, in order; a failing or killed hook is a result, not an error.
    pub fn run(&self, event: &str, payload: &Value) -> Vec<HookRun> {
        let body = payload.to_string();
        self.entries
            .iter()
            .filter(|e| e.event == event && matches(event, &e.matcher))
            .map(|e| run_command(&e.command, &body, e.timeout))
            .collect()
    }
}

fn matches(event: &str, matcher: &str) -> bool {
    if event == "PreToolUse" {
        matcher == "AskUserQuestion"
    } else {
        matcher == "*"
    }
}

/// Value of `name` in the first `?…` / `&…` query found in `text`.
fn query_param(text: &str, name: &str) -> Option<String> {
    let query = text.split_once('?')?.1;
    let query = query.split(['\'', '"', ' ']).next()?;
    query.split('&').find_map(|pair| {
        let (k, v) = pair.split_once('=')?;
        (k == name).then(|| v.to_string())
    })
}

/// How long output is still collected after the hook's shell has exited: a
/// backgrounded grandchild may keep the pipe open, and must not hold us up.
const DRAIN_GRACE: Duration = Duration::from_millis(300);

fn run_command(command: &str, stdin: &str, timeout_secs: u64) -> HookRun {
    let failed = HookRun {
        exit_code: -1,
        stdout: String::new(),
    };
    // Own process group, so a timeout can take the grandchildren down with the shell.
    let Ok(mut child) = Command::new("sh")
        .arg("-c")
        .arg(command)
        .process_group(0)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    else {
        return failed;
    };
    let pid = child.id();
    // Feed stdin and drain stdout on threads so a slow hook can't block us.
    let mut input = child.stdin.take();
    let body = stdin.to_string();
    let _writer = std::thread::spawn(move || {
        if let Some(mut i) = input.take() {
            let _ = i.write_all(body.as_bytes());
        }
    });
    let mut out = child.stdout.take();
    let (tx, rx) = mpsc::channel::<Vec<u8>>();
    let _reader = std::thread::spawn(move || {
        let Some(o) = out.as_mut() else { return };
        let mut buf = [0u8; 4096];
        while let Ok(n) = o.read(&mut buf) {
            if n == 0 || tx.send(buf[..n].to_vec()).is_err() {
                break;
            }
        }
    });
    let deadline = Instant::now() + Duration::from_secs(timeout_secs);
    let code = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status.code().unwrap_or(-1),
            Ok(None) if Instant::now() >= deadline => {
                // Not `unsafe { killpg }`: the workspace forbids it.
                let _ = Command::new("kill")
                    .args(["-KILL", &format!("-{pid}")])
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .status();
                let _ = child.kill();
                let _ = child.wait();
                return failed;
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(10)),
            Err(_) => return failed,
        }
    };
    let mut bytes = Vec::new();
    let end = Instant::now() + DRAIN_GRACE;
    while let Some(left) = end.checked_duration_since(Instant::now()) {
        match rx.recv_timeout(left) {
            Ok(chunk) => bytes.extend(chunk),
            Err(_) => break,
        }
    }
    HookRun {
        exit_code: code,
        stdout: String::from_utf8_lossy(&bytes).into_owned(),
    }
}

/// Payload builders, one per event (shapes in contracts/fake-claude.md).
pub mod payload {
    use super::*;

    fn base(event: &str, session: &str, cwd: &str) -> Value {
        json!({"hook_event_name": event, "session_id": session, "cwd": cwd})
    }

    fn with(mut v: Value, extra: Value) -> Value {
        if let (Some(a), Some(b)) = (v.as_object_mut(), extra.as_object()) {
            a.extend(b.clone());
        }
        v
    }

    /// `source` is `startup` or `resume`.
    pub fn session_start(session: &str, cwd: &str, source: &str) -> Value {
        with(
            base("SessionStart", session, cwd),
            json!({"source": source}),
        )
    }

    pub fn user_prompt_submit(session: &str, cwd: &str, prompt: &str) -> Value {
        with(
            base("UserPromptSubmit", session, cwd),
            json!({"prompt": prompt}),
        )
    }

    /// `options` are `(label, description)` pairs.
    pub fn pre_tool_use_ask(
        session: &str,
        cwd: &str,
        question: &str,
        header: &str,
        options: &[(&str, &str)],
    ) -> Value {
        let options = options
            .iter()
            .map(|(l, d)| json!({"label": l, "description": d}))
            .collect();
        pre_tool_use_ask_with(session, cwd, question, header, options)
    }

    /// Like [`pre_tool_use_ask`] with the options already as JSON objects (they
    /// may carry a `preview`).
    pub fn pre_tool_use_ask_with(
        session: &str,
        cwd: &str,
        question: &str,
        header: &str,
        options: Vec<Value>,
    ) -> Value {
        with(
            base("PreToolUse", session, cwd),
            json!({"tool_name": "AskUserQuestion", "tool_input": {"questions": [{
                "question": question, "header": header,
                "multiSelect": false, "options": options}]}}),
        )
    }

    pub fn post_tool_use(
        session: &str,
        cwd: &str,
        tool_name: &str,
        tool_input: Value,
        tool_response: Value,
    ) -> Value {
        with(
            base("PostToolUse", session, cwd),
            json!({"tool_name": tool_name, "tool_input": tool_input,
                   "tool_response": tool_response}),
        )
    }

    pub fn stop(session: &str, cwd: &str, last_assistant_message: &str) -> Value {
        with(
            base("Stop", session, cwd),
            json!({"last_assistant_message": last_assistant_message}),
        )
    }

    pub fn stop_failure(session: &str, cwd: &str, error: &str) -> Value {
        with(base("StopFailure", session, cwd), json!({"error": error}))
    }

    pub fn notification_idle(session: &str, cwd: &str, message: &str) -> Value {
        with(
            base("Notification", session, cwd),
            json!({"notification_type": "idle_prompt", "message": message}),
        )
    }

    pub fn session_end(session: &str, cwd: &str, reason: &str) -> Value {
        with(base("SessionEnd", session, cwd), json!({"reason": reason}))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use delilah_agents::claude::{Permission, settings};

    /// Settings as the daemon writes them, with every command replaced by one that
    /// appends its stdin to `<dir>/<Event>.json` (PreToolUse also prints `ANSWER`).
    fn fake_settings(dir: &Path) -> std::path::PathBuf {
        let mut s = settings(
            "http://127.0.0.1:1",
            "KEY-1",
            "sess-1",
            Path::new("/bin/true"),
            None,
            Permission::Default,
            &[],
        );
        for (event, groups) in s["hooks"].as_object_mut().unwrap() {
            for g in groups.as_array_mut().unwrap() {
                for h in g["hooks"].as_array_mut().unwrap() {
                    let extra = if event == "PreToolUse" {
                        "; echo ANSWER"
                    } else {
                        ""
                    };
                    h["command"] = json!(format!("cat >> {}/{event}.json{extra}", dir.display()));
                }
            }
        }
        let path = dir.join("settings.json");
        std::fs::write(&path, s.to_string()).unwrap();
        path
    }

    fn with_command(dir: &Path, command: &str, timeout: Option<u64>) -> Hooks {
        let mut hook = json!({"type": "command", "command": command});
        if let Some(t) = timeout {
            hook["timeout"] = json!(t);
        }
        let s = json!({"hooks": {"Stop": [{"matcher": "*", "hooks": [hook]}]}});
        let p = dir.join("s.json");
        std::fs::write(&p, s.to_string()).unwrap();
        Hooks::load(&p).unwrap()
    }

    #[test]
    fn task_comes_from_hook_url() {
        let dir = tempfile::tempdir().unwrap();
        let s = settings(
            "http://127.0.0.1:1",
            "KEY-1",
            "sess-1",
            Path::new("/bin/true"),
            None,
            Permission::Default,
            &[],
        );
        let p = dir.path().join("s.json");
        std::fs::write(&p, s.to_string()).unwrap();
        assert_eq!(Hooks::load(&p).unwrap().task().as_deref(), Some("KEY-1"));
    }

    #[test]
    fn run_feeds_payload_to_every_matching_command() {
        let dir = tempfile::tempdir().unwrap();
        let hooks = Hooks::load(&fake_settings(dir.path())).unwrap();
        let p = payload::stop("sess-1", "/w", "done");
        let runs = hooks.run("Stop", &p);
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].exit_code, 0);
        let got = std::fs::read_to_string(dir.path().join("Stop.json")).unwrap();
        assert_eq!(serde_json::from_str::<Value>(&got).unwrap(), p);
    }

    #[test]
    fn pre_tool_use_runs_ask_matcher_and_returns_stdout() {
        let dir = tempfile::tempdir().unwrap();
        let hooks = Hooks::load(&fake_settings(dir.path())).unwrap();
        let p = payload::pre_tool_use_ask("sess-1", "/w", "Q?", "H", &[("a", "A")]);
        let runs = hooks.run("PreToolUse", &p);
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].stdout.trim(), "ANSWER");
        assert!(dir.path().join("PreToolUse.json").exists());
    }

    #[test]
    fn unknown_event_runs_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let hooks = Hooks::load(&fake_settings(dir.path())).unwrap();
        assert!(hooks.run("Nope", &json!({})).is_empty());
    }

    #[test]
    fn non_zero_exit_is_recorded() {
        let dir = tempfile::tempdir().unwrap();
        let hooks = with_command(dir.path(), "cat >/dev/null; exit 3", None);
        assert_eq!(hooks.run("Stop", &json!({}))[0].exit_code, 3);
    }

    #[test]
    fn timeout_kills_and_records_minus_one() {
        let dir = tempfile::tempdir().unwrap();
        let hooks = with_command(dir.path(), "exec sleep 30", Some(1));
        let t = Instant::now();
        assert_eq!(hooks.run("Stop", &json!({}))[0].exit_code, -1);
        assert!(t.elapsed() < Duration::from_secs(10));
    }

    fn alive(pid: &str) -> bool {
        Command::new("kill")
            .args(["-0", pid])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }

    #[test]
    fn backgrounded_grandchild_holding_stdout_does_not_hang() {
        let dir = tempfile::tempdir().unwrap();
        let pidfile = dir.path().join("pid");
        let cmd = format!("sleep 30 & echo $! > {}; echo hi", pidfile.display());
        let hooks = with_command(dir.path(), &cmd, Some(5));
        let t = Instant::now();
        let r = hooks.run("Stop", &json!({}));
        assert!(t.elapsed() < Duration::from_secs(3), "{:?}", t.elapsed());
        assert_eq!((r[0].exit_code, r[0].stdout.trim()), (0, "hi"));
        let _ = Command::new("kill")
            .arg(std::fs::read_to_string(&pidfile).unwrap().trim())
            .status();
    }

    #[test]
    fn timeout_kills_the_whole_process_group() {
        let dir = tempfile::tempdir().unwrap();
        let pidfile = dir.path().join("pid");
        let cmd = format!("sleep 30 & echo $! > {}; wait", pidfile.display());
        let hooks = with_command(dir.path(), &cmd, Some(1));
        let t = Instant::now();
        assert_eq!(hooks.run("Stop", &json!({}))[0].exit_code, -1);
        assert!(t.elapsed() < Duration::from_secs(5), "{:?}", t.elapsed());
        let pid = std::fs::read_to_string(&pidfile)
            .unwrap()
            .trim()
            .to_string();
        let end = Instant::now() + Duration::from_secs(3);
        while alive(&pid) && Instant::now() < end {
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(!alive(&pid), "grandchild {pid} survived the timeout");
    }

    #[test]
    fn payload_shapes() {
        let p = payload::session_start("s", "/w", "resume");
        assert_eq!(
            p,
            json!({"hook_event_name":"SessionStart","session_id":"s","cwd":"/w","source":"resume"})
        );
        assert_eq!(payload::user_prompt_submit("s", "/w", "hi")["prompt"], "hi");
        let q = payload::pre_tool_use_ask("s", "/w", "Q?", "H", &[("a", "A")]);
        assert_eq!(q["tool_name"], "AskUserQuestion");
        let question = &q["tool_input"]["questions"][0];
        assert_eq!(question["question"], "Q?");
        assert_eq!(question["header"], "H");
        assert_eq!(question["multiSelect"], false);
        assert_eq!(
            question["options"][0],
            json!({"label":"a","description":"A"})
        );
        let post = payload::post_tool_use("s", "/w", "Bash", json!({"c":1}), json!({"r":2}));
        assert_eq!(post["tool_name"], "Bash");
        assert_eq!(post["tool_input"], json!({"c":1}));
        assert_eq!(post["tool_response"], json!({"r":2}));
        assert_eq!(payload::stop("s", "/w", "m")["last_assistant_message"], "m");
        assert_eq!(payload::stop_failure("s", "/w", "e")["error"], "e");
        let n = payload::notification_idle("s", "/w", "m");
        assert_eq!(n["notification_type"], "idle_prompt");
        assert_eq!(n["message"], "m");
        assert_eq!(payload::session_end("s", "/w", "exit")["reason"], "exit");
        assert_eq!(n["hook_event_name"], "Notification");
        assert_eq!(n["cwd"], "/w");
    }
}
