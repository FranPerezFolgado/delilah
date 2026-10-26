//! Chat mode of the fake Claude Code: `claude -p --input-format stream-json
//! --output-format stream-json …` as the daemon runs it (`claude::stream_command`), with
//! the wire shapes of docs/spikes/2026-10-24-claude-stream.md. Each user line on stdin
//! plays the next turn of the scenario's `turns` (picked at the first line: a
//! `[fake:<name>]` marker in it, else `<task>.json`, else `default.json`); the hooks fire
//! as in the terminal (SessionStart, UserPromptSubmit, Stop, SessionEnd) and the
//! conversation is appended to the transcript where Claude keeps it.

use std::collections::VecDeque;
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde_json::{Value, json};

use super::hooks::{Hooks, payload};
use super::record::{HookResult, Record};
use super::run::write_transcript;
use super::scenario::{self, AutoMode, Say, Scenario, StreamStep, Tool};

/// What came in on stdin.
#[derive(Debug, Clone, PartialEq)]
enum In {
    User(String),
    Interrupt(String),
    /// A `control_response`: its request id and inner `response`.
    Response(String, Value),
    Other,
}

fn classify(line: &str) -> In {
    let Ok(v) = serde_json::from_str::<Value>(line) else {
        return In::Other;
    };
    match v["type"].as_str() {
        Some("user") => {
            let text = match &v["message"]["content"] {
                Value::String(s) => s.clone(),
                Value::Array(blocks) => blocks
                    .iter()
                    .filter_map(|b| b["text"].as_str())
                    .collect::<Vec<_>>()
                    .join("\n"),
                _ => String::new(),
            };
            In::User(text)
        }
        Some("control_request") if v["request"]["subtype"] == "interrupt" => {
            In::Interrupt(v["request_id"].as_str().unwrap_or_default().to_string())
        }
        Some("control_response") => In::Response(
            v["response"]["request_id"]
                .as_str()
                .unwrap_or_default()
                .to_string(),
            v["response"]["response"].clone(),
        ),
        _ => In::Other,
    }
}

/// How a step or turn ended.
enum Flow {
    Next,
    /// An interrupt came: the turn ends at once, with no `Stop`.
    Interrupted(String),
    Exit(i32),
}

struct Runner<'a, R, W> {
    record: &'a mut Record,
    hooks: &'a Hooks,
    fake_dir: Option<&'a Path>,
    input: &'a mut R,
    out: &'a mut W,
    transcript: Option<PathBuf>,
    scenario: Option<Scenario>,
    turn: usize,
    /// User lines that came while a turn waited for something else.
    queued: VecDeque<String>,
}

static SEQ: AtomicU64 = AtomicU64::new(0);

/// An id unique across fake processes (a resumed session appends to the same transcript).
fn unique(prefix: &str) -> String {
    let n = SEQ.fetch_add(1, Ordering::Relaxed);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or_default();
    format!("{prefix}{:x}{:08x}{n:x}", std::process::id(), nanos)
}

fn uuid() -> String {
    let n = SEQ.fetch_add(1, Ordering::Relaxed);
    let d = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    format!(
        "{:08x}-{:04x}-4{:03x}-8{:03x}-{:012x}",
        d.as_secs() as u32,
        std::process::id() & 0xffff,
        n & 0xfff,
        (d.subsec_nanos() >> 20) & 0xfff,
        (u64::from(d.subsec_nanos()) << 16 | (n & 0xffff)) & 0xffff_ffff_ffff
    )
}

/// The current time as RFC 3339 UTC with milliseconds, as Claude stamps transcript lines.
fn now() -> String {
    let d = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let secs = d.as_secs() as i64;
    let (days, rem) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    // Days to civil date (H. Hinnant).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}.{:03}Z",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60,
        d.subsec_millis()
    )
}

/// Word-sized pieces of `text`, each keeping its trailing space.
fn chunks(text: &str) -> Vec<&str> {
    text.split_inclusive(' ').collect()
}

impl<R: BufRead, W: Write> Runner<'_, R, W> {
    fn save(&self) {
        if let Some(dir) = self.fake_dir
            && let Err(e) = self.record.save(dir)
        {
            eprintln!("fake-claude: cannot save record: {e}");
        }
    }

    fn hook(&mut self, event: &str, payload: &Value) {
        for r in self.hooks.run(event, payload) {
            self.record.hooks.push(HookResult {
                event: event.to_string(),
                exit_code: r.exit_code,
            });
        }
    }

    fn emit(&mut self, v: &Value) {
        let _ = writeln!(self.out, "{v}");
        let _ = self.out.flush();
    }

    fn append(&mut self, kind: &str, message: &Value) {
        let Some(path) = &self.transcript else { return };
        let line = json!({
            "type": kind,
            "uuid": uuid(),
            "timestamp": now(),
            "sessionId": self.record.session_id,
            "cwd": self.record.cwd,
            "isSidechain": false,
            "message": message,
        });
        if let Ok(mut f) = std::fs::OpenOptions::new().append(true).open(path) {
            let _ = writeln!(f, "{line}");
        }
    }

    fn read(&mut self) -> Option<In> {
        let mut line = String::new();
        match self.input.read_line(&mut line) {
            Ok(0) | Err(_) => None,
            Ok(_) => Some(classify(line.trim_end())),
        }
    }

    fn end(&mut self, reason: &str, code: i32) -> i32 {
        let p = payload::session_end(&self.record.session_id, &self.record.cwd, reason);
        self.hook("SessionEnd", &p);
        self.record.exited = Some(code);
        self.save();
        code
    }

    fn steps_for(&mut self, text: &str) -> Vec<StreamStep> {
        if self.scenario.is_none() {
            let picked = self
                .fake_dir
                .map(|dir| scenario::select(dir, Some(text), self.record.task.as_deref()));
            match picked {
                Some(Ok((name, s))) => {
                    self.record.scenario = name;
                    self.scenario = Some(s);
                }
                Some(Err(e)) => {
                    eprintln!("fake-claude: {e}");
                    self.scenario = Some(Scenario {
                        steps: vec![],
                        turns: vec![],
                    });
                }
                None => {
                    self.scenario = Some(Scenario {
                        steps: vec![],
                        turns: vec![],
                    })
                }
            }
        }
        let turn = self.turn;
        self.turn += 1;
        self.scenario
            .as_ref()
            .and_then(|s| s.turns.get(turn).cloned())
            .unwrap_or_else(|| {
                vec![StreamStep::Say(Say {
                    text: format!("Echo: {text}"),
                    delay_ms: 0,
                })]
            })
    }

    fn say(&mut self, say: &Say) -> String {
        let id = unique("msg_");
        self.emit(
            &json!({"type": "stream_event", "event": {"type": "message_start",
            "message": {"id": id, "role": "assistant", "content": []}}}),
        );
        self.emit(
            &json!({"type": "stream_event", "event": {"type": "content_block_start",
            "index": 0, "content_block": {"type": "text", "text": ""}}}),
        );
        for piece in chunks(&say.text) {
            self.emit(
                &json!({"type": "stream_event", "event": {"type": "content_block_delta",
                "index": 0, "delta": {"type": "text_delta", "text": piece}}}),
            );
            if say.delay_ms > 0 {
                std::thread::sleep(Duration::from_millis(say.delay_ms));
            }
        }
        let message = json!({"id": id, "type": "message", "role": "assistant",
            "content": [{"type": "text", "text": say.text}]});
        // In the transcript before the message ends, so the view finds it when told.
        self.append("assistant", &message);
        self.emit(&json!({"type": "assistant", "message": message}));
        self.emit(
            &json!({"type": "stream_event", "event": {"type": "content_block_stop", "index": 0}}),
        );
        self.emit(&json!({"type": "stream_event", "event": {"type": "message_stop"}}));
        say.text.clone()
    }

    fn tool(&mut self, tool: &Tool) -> Flow {
        let id = unique("msg_");
        let tool_use_id = unique("toolu_");
        let message = json!({"id": id, "type": "message", "role": "assistant",
            "content": [{"type": "tool_use", "id": tool_use_id, "name": tool.name, "input": tool.input}]});
        self.emit(
            &json!({"type": "stream_event", "event": {"type": "message_start",
            "message": {"id": id, "role": "assistant", "content": []}}}),
        );
        self.append("assistant", &message);
        self.emit(&json!({"type": "assistant", "message": message}));
        self.emit(&json!({"type": "stream_event", "event": {"type": "message_stop"}}));

        // Mirrors the spike (`docs/spikes/2026-10-26-chat-permission-mode.md`): under
        // `--permission-mode auto` a tool marked `auto: allow` runs without asking.
        let auto_allows = tool.auto == AutoMode::Allow
            && self
                .record
                .args
                .windows(2)
                .any(|w| w[0] == "--permission-mode" && w[1] == "auto");
        let (allowed, text) = if tool.ask && !auto_allows {
            let request_id = unique("perm-");
            let suggestions = tool.suggestions.clone().unwrap_or_else(|| {
                let mut rule = json!({"toolName": tool.name});
                if let Some(c) = tool.input["command"].as_str() {
                    rule["ruleContent"] = json!(c);
                }
                json!([{"type": "addRules", "behavior": "allow",
                    "destination": "localSettings", "rules": [rule]}])
            });
            self.emit(&json!({"type": "control_request", "request_id": request_id,
                "request": {"subtype": "can_use_tool", "tool_name": tool.name,
                    "display_name": tool.name, "tool_use_id": tool_use_id,
                    "input": tool.input, "permission_suggestions": suggestions}}));
            self.save();
            let response = loop {
                match self.read() {
                    None => return Flow::Exit(self.end("hangup", 0)),
                    Some(In::Response(rid, r)) if rid == request_id => break r,
                    Some(In::Interrupt(rid)) => {
                        self.record.permissions.push(json!({"tool": tool.name,
                            "input": tool.input, "response": null}));
                        return Flow::Interrupted(rid);
                    }
                    Some(In::User(text)) => self.queued.push_back(text),
                    Some(_) => {}
                }
            };
            self.record
                .permissions
                .push(json!({"tool": tool.name, "input": tool.input, "response": response}));
            self.save();
            if response["behavior"] == "allow" {
                (true, tool.output.clone())
            } else {
                let why = response["message"].as_str().unwrap_or("denied");
                (false, why.to_string())
            }
        } else {
            (true, tool.output.clone())
        };
        let result = json!({"role": "user", "content": [{"type": "tool_result",
            "tool_use_id": tool_use_id, "content": text, "is_error": !allowed}]});
        self.append("user", &result);
        self.emit(&json!({"type": "user", "message": result}));
        Flow::Next
    }

    fn turn(&mut self, text: &str) -> Flow {
        let (session, cwd) = (self.record.session_id.clone(), self.record.cwd.clone());
        let steps = self.steps_for(text);
        self.emit(&json!({"type": "system", "subtype": "init", "session_id": session, "cwd": cwd}));
        self.append("user", &json!({"role": "user", "content": text}));
        self.record.inputs.push(text.to_string());
        self.hook(
            "UserPromptSubmit",
            &payload::user_prompt_submit(&session, &cwd, text),
        );
        self.save();
        let mut last = String::new();
        for step in &steps {
            match step {
                StreamStep::Say(say) => last = self.say(say),
                StreamStep::Tool(tool) => match self.tool(tool) {
                    Flow::Next => {}
                    other => return other,
                },
                StreamStep::SleepMs(ms) => std::thread::sleep(Duration::from_millis(*ms)),
                StreamStep::Exit(code) => return Flow::Exit(self.end("exit", *code)),
            }
            self.record.steps_done += 1;
            self.save();
        }
        self.emit(
            &json!({"type": "rate_limit_event", "rate_limit_info": {"status": "allowed",
            "unifiedWindows": {"five_hour": {"resetsAt": 1_792_872_000, "utilization": 0.1}}}}),
        );
        self.emit(
            &json!({"type": "result", "subtype": "success", "is_error": false, "result": last}),
        );
        self.hook("Stop", &payload::stop(&session, &cwd, &last));
        self.save();
        Flow::Next
    }

    fn interrupted(&mut self, request_id: &str) {
        self.emit(
            &json!({"type": "control_response", "response": {"subtype": "success",
            "request_id": request_id, "response": {"still_queued": []}}}),
        );
        self.emit(&json!({"type": "user", "message": {"role": "user",
            "content": [{"type": "text", "text": "[Request interrupted by user]"}]}}));
        self.emit(
            &json!({"type": "result", "subtype": "error_during_execution", "is_error": true,
            "errors": ["[ede_diagnostic] result_type=user"]}),
        );
    }
}

/// Plays a chat-mode session until stdin closes; returns the exit code. `record` must
/// already hold the session facts.
pub fn run<R: BufRead, W: Write>(
    record: &mut Record,
    hooks: &Hooks,
    fake_dir: Option<&Path>,
    input: &mut R,
    out: &mut W,
) -> i32 {
    let transcript = write_transcript(record, fake_dir);
    let mut r = Runner {
        record,
        hooks,
        fake_dir,
        input,
        out,
        transcript,
        scenario: None,
        turn: 0,
        queued: VecDeque::new(),
    };
    let source = if r.record.mode == "resume" {
        "resume"
    } else {
        "startup"
    };
    let start = payload::session_start(&r.record.session_id, &r.record.cwd, source);
    r.hook("SessionStart", &start);
    r.save();
    loop {
        let next = match r.queued.pop_front() {
            Some(text) => Some(In::User(text)),
            None => r.read(),
        };
        match next {
            None => return r.end("hangup", 0),
            Some(In::User(text)) => match r.turn(&text) {
                Flow::Next => {}
                Flow::Interrupted(rid) => r.interrupted(&rid),
                Flow::Exit(code) => return code,
            },
            // Nothing running: the interrupt has nothing to stop.
            Some(In::Interrupt(rid)) => {
                r.emit(
                    &json!({"type": "control_response", "response": {"subtype": "success",
                    "request_id": rid, "response": {"still_queued": []}}}),
                );
            }
            Some(_) => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use std::io::Cursor;

    fn record() -> Record {
        Record {
            args: vec![],
            mode: "new".into(),
            session_id: "s1".into(),
            task: None,
            cwd: "/tmp".into(),
            env: BTreeMap::new(),
            settings: String::new(),
            prompt: None,
            scenario: String::new(),
            steps_done: 0,
            hooks: vec![],
            answers: vec![],
            inputs: vec![],
            permissions: vec![],
            exited: None,
        }
    }

    fn lines(out: &[u8]) -> Vec<Value> {
        String::from_utf8_lossy(out)
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect()
    }

    fn scenario_dir(json: &str) -> tempfile::TempDir {
        let d = tempfile::tempdir().unwrap();
        std::fs::create_dir(d.path().join("scenarios")).unwrap();
        std::fs::write(d.path().join("scenarios/default.json"), r#"{"steps":[]}"#).unwrap();
        std::fs::write(d.path().join("scenarios/chat.json"), json).unwrap();
        d
    }

    #[test]
    fn classifies_what_the_daemon_writes() {
        assert_eq!(
            classify(r#"{"type":"user","message":{"role":"user","content":"hola"}}"#),
            In::User("hola".into())
        );
        assert_eq!(
            classify(
                r#"{"type":"control_request","request_id":"i1","request":{"subtype":"interrupt"}}"#
            ),
            In::Interrupt("i1".into())
        );
        assert_eq!(
            classify(
                r#"{"type":"control_response","response":{"subtype":"success","request_id":"p","response":{"behavior":"deny"}}}"#
            ),
            In::Response("p".into(), json!({"behavior": "deny"}))
        );
        assert_eq!(classify("nope"), In::Other);
    }

    #[test]
    fn now_is_rfc3339() {
        let t = now();
        assert_eq!(t.len(), 24, "{t}");
        assert!(t.ends_with('Z') && t.as_bytes()[10] == b'T', "{t}");
    }

    #[test]
    fn a_turn_streams_text_then_the_message_and_a_result() {
        let d = scenario_dir(r#"{"turns":[[{"say":{"text":"Hola mundo"}}]]}"#);
        let mut rec = record();
        let mut input = Cursor::new(
            "{\"type\":\"user\",\"message\":{\"role\":\"user\",\"content\":\"hi [fake:chat]\"}}\n",
        );
        let mut out = Vec::new();
        let code = run(
            &mut rec,
            &Hooks::default(),
            Some(d.path()),
            &mut input,
            &mut out,
        );
        assert_eq!(code, 0);
        assert_eq!(rec.scenario, "chat");
        let out = lines(&out);
        assert_eq!(out[0]["subtype"], "init");
        let deltas: Vec<&str> = out
            .iter()
            .filter_map(|v| v.pointer("/event/delta/text").and_then(Value::as_str))
            .collect();
        assert_eq!(deltas, ["Hola ", "mundo"]);
        assert!(out.iter().any(
            |v| v["type"] == "assistant" && v["message"]["content"][0]["text"] == "Hola mundo"
        ));
        assert_eq!(out.last().unwrap()["type"], "result");
    }

    /// Runs one turn with a single `Bash` tool and returns the `can_use_tool` count.
    fn asks_for(tool_extra: &str, mode: Option<&str>) -> usize {
        let d = scenario_dir(&format!(
            r#"{{"turns":[[{{"tool":{{"name":"Bash","input":{{"command":"ls"}},"output":"ok"{tool_extra}}}}}]]}}"#
        ));
        let mut rec = record();
        if let Some(m) = mode {
            rec.args = vec!["--permission-mode".into(), m.into()];
        }
        let user =
            "{\"type\":\"user\",\"message\":{\"role\":\"user\",\"content\":\"[fake:chat]\"}}\n";
        let mut out = Vec::new();
        run(
            &mut rec,
            &Hooks::default(),
            Some(d.path()),
            &mut Cursor::new(user),
            &mut out,
        );
        lines(&out)
            .iter()
            .filter(|v| v["type"] == "control_request")
            .count()
    }

    #[test]
    fn auto_mode_skips_can_use_tool_for_an_allowed_tool_only() {
        assert_eq!(asks_for("", Some("auto")), 0);
        assert_eq!(asks_for("", None), 1);
        assert_eq!(asks_for("", Some("plan")), 1);
        assert_eq!(asks_for(r#","auto":"ask""#, Some("auto")), 1);
        assert_eq!(asks_for(r#","auto":"ask""#, None), 1);
    }

    #[test]
    fn a_tool_asks_can_use_tool_with_the_exact_command_as_suggestion() {
        let d = scenario_dir(
            r#"{"turns":[[{"tool":{"name":"Bash","input":{"command":"ls -la"},"output":"total 0"}}]]}"#,
        );
        let mut rec = record();
        let user =
            "{\"type\":\"user\",\"message\":{\"role\":\"user\",\"content\":\"[fake:chat]\"}}\n";
        let mut out = Vec::new();
        run(
            &mut rec,
            &Hooks::default(),
            Some(d.path()),
            &mut Cursor::new(user),
            &mut out,
        );
        let req = lines(&out)
            .into_iter()
            .find(|v| v["type"] == "control_request")
            .unwrap();
        assert_eq!(req["request"]["tool_name"], "Bash");
        assert_eq!(
            req["request"]["permission_suggestions"][0]["rules"][0]["ruleContent"],
            "ls -la"
        );
        assert!(req["request_id"].as_str().unwrap().starts_with("perm-"));
        // Stdin closed while it waited: nothing answered, the session ended.
        assert!(rec.permissions.is_empty());
        assert_eq!(rec.exited, Some(0));
    }
}
