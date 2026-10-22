//! The step runner of a fake session: plays a [`Scenario`] on a terminal (any
//! `BufRead` / `Write`), reporting through the settings' hooks and keeping the
//! [`Record`] up to date.

use std::io::{BufRead, Write};
use std::path::Path;
use std::time::Duration;

use serde_json::{json, Value};

use super::hooks::{payload, HookRun, Hooks};
use super::record::{HookResult, Record};
use super::scenario::{Ask, Scenario, Step};

/// The folder-trust dialog, as in contracts/fake-claude.md.
fn trust_dialog(cwd: &str) -> String {
    format!("Do you trust the files in this folder?\n{cwd}\n❯ 1. Yes, proceed\n  2. No, exit\n")
}

struct Runner<'a, R, W> {
    record: &'a mut Record,
    hooks: &'a Hooks,
    fake_dir: Option<&'a Path>,
    input: &'a mut R,
    out: &'a mut W,
}

/// How a step ended: go on, or leave with an exit code.
enum Flow {
    Next,
    Exit(i32),
}

impl<R: BufRead, W: Write> Runner<'_, R, W> {
    fn save(&self) {
        if let Some(dir) = self.fake_dir
            && let Err(e) = self.record.save(dir)
        {
            eprintln!("fake-claude: cannot save record: {e}");
        }
    }

    /// Runs the hooks of `event` and notes them in the record.
    fn hook(&mut self, event: &str, payload: &Value) -> Vec<HookRun> {
        let runs = self.hooks.run(event, payload);
        for r in &runs {
            self.record
                .hooks
                .push(HookResult { event: event.to_string(), exit_code: r.exit_code });
        }
        runs
    }

    fn print(&mut self, text: &str) {
        let _ = self.out.write_all(text.as_bytes());
        let _ = self.out.flush();
    }

    /// One line from the terminal; `None` on EOF or a read error (hang-up).
    fn read_line(&mut self) -> Option<String> {
        let mut line = String::new();
        match self.input.read_line(&mut line) {
            Ok(0) | Err(_) => None,
            Ok(_) => Some(line.trim_end_matches(['\r', '\n']).to_string()),
        }
    }

    fn end(&mut self, reason: &str, code: i32) -> i32 {
        let p = payload::session_end(&self.record.session_id, &self.record.cwd, reason);
        self.hook("SessionEnd", &p);
        self.record.exited = Some(code);
        self.save();
        code
    }

    fn step(&mut self, step: &Step) -> Flow {
        let (session, cwd) = (self.record.session_id.clone(), self.record.cwd.clone());
        match step {
            Step::TrustDialog(show) => {
                if *show {
                    self.print(&trust_dialog(&cwd));
                    match self.read_line() {
                        None => return Flow::Exit(self.end("hangup", 0)),
                        Some(l) if l.trim() == "2" => {
                            return Flow::Exit(self.end("trust_declined", 1));
                        }
                        Some(_) => {}
                    }
                }
            }
            Step::Print(text) => self.print(&format!("{text}\n")),
            Step::Hook(event) => {
                let p = json!({"hook_event_name": event, "session_id": session, "cwd": cwd});
                self.hook(event, &p);
            }
            Step::SleepMs(ms) => std::thread::sleep(Duration::from_millis(*ms)),
            Step::Ask(ask) => return self.ask(ask),
            Step::Stop(message) => {
                self.hook("Stop", &payload::stop(&session, &cwd, message));
            }
            Step::WaitInput(_) => match self.read_line() {
                None => return Flow::Exit(self.end("hangup", 0)),
                Some(line) => {
                    self.hook("UserPromptSubmit", &payload::user_prompt_submit(&session, &cwd, &line));
                    self.record.inputs.push(line);
                }
            },
            Step::Fail(message) => {
                self.print(&format!("{message}\n"));
                self.hook("StopFailure", &payload::stop_failure(&session, &cwd, message));
                return Flow::Exit(self.end("error", 1));
            }
            Step::Exit(code) => return Flow::Exit(self.end("exit", *code)),
        }
        Flow::Next
    }

    fn ask(&mut self, ask: &Ask) -> Flow {
        let (session, cwd) = (self.record.session_id.clone(), self.record.cwd.clone());
        let options: Vec<(&str, &str)> = ask.options.iter().map(|o| (o.as_str(), "")).collect();
        let pre = payload::pre_tool_use_ask(&session, &cwd, &ask.question, &ask.header, &options);
        let tool_input = pre["tool_input"].clone();
        let runs = self.hook("PreToolUse", &pre);
        let answers = runs.iter().find_map(|r| {
            let v: Value = serde_json::from_str(r.stdout.trim()).ok()?;
            let a = v.pointer("/hookSpecificOutput/updatedInput/answers")?;
            Some(a.clone())
        });
        let response = match answers {
            Some(a) => {
                self.record.answers.push(a.clone());
                a
            }
            None => {
                // Released without an answer: ask on the terminal, as Claude would.
                self.record.answers.push(Value::Null);
                let mut text = format!("{}\n{}\n", ask.header, ask.question);
                for (i, o) in ask.options.iter().enumerate() {
                    text.push_str(&format!("  {}. {o}\n", i + 1));
                }
                self.print(&text);
                match self.read_line() {
                    None => return Flow::Exit(self.end("hangup", 0)),
                    Some(line) => {
                        self.record.inputs.push(line.clone());
                        json!({ask.question.clone(): line})
                    }
                }
            }
        };
        let post = payload::post_tool_use(&session, &cwd, "AskUserQuestion", tool_input, response);
        self.hook("PostToolUse", &post);
        Flow::Next
    }
}

/// Plays `scenario` and returns the exit code. `record` must already hold the
/// session facts; it is saved to `fake_dir` (when given) after each step.
pub fn run<R: BufRead, W: Write>(
    record: &mut Record,
    scenario: &Scenario,
    hooks: &Hooks,
    fake_dir: Option<&Path>,
    input: &mut R,
    out: &mut W,
) -> i32 {
    let mut r = Runner { record, hooks, fake_dir, input, out };
    let source = if r.record.mode == "resume" { "resume" } else { "startup" };
    let start = payload::session_start(&r.record.session_id, &r.record.cwd, source);
    r.hook("SessionStart", &start);
    r.save();

    for step in &scenario.steps {
        let flow = r.step(step);
        r.record.steps_done += 1;
        r.save();
        if let Flow::Exit(code) = flow {
            return code;
        }
    }

    // Idle, like Claude waiting for a prompt: answer every line typed.
    while let Some(line) = r.read_line() {
        if line.trim().is_empty() {
            continue;
        }
        let (session, cwd) = (r.record.session_id.clone(), r.record.cwd.clone());
        r.record.inputs.push(line.clone());
        r.hook("UserPromptSubmit", &payload::user_prompt_submit(&session, &cwd, &line));
        r.hook("Stop", &payload::stop(&session, &cwd, &format!("Echo: {line}")));
        r.save();
    }
    r.end("hangup", 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use delilah_agents::claude::trust_dialog_selection;
    use std::collections::BTreeMap;
    use std::io::Cursor;

    const EVENTS: [&str; 8] = [
        "SessionStart", "UserPromptSubmit", "PreToolUse", "PostToolUse", "Stop",
        "StopFailure", "Notification", "SessionEnd",
    ];

    /// Settings whose hooks append their stdin (one line each) to `<dir>/<Event>.log`;
    /// `PreToolUse` then prints the contents of `<dir>/answer`, if any.
    fn settings(dir: &Path) -> Hooks {
        let mut hooks = serde_json::Map::new();
        for ev in EVENTS {
            let extra = if ev == "PreToolUse" { format!("; cat {}/answer 2>/dev/null; true", dir.display()) } else { String::new() };
            let command = format!("{{ cat; echo; }} >> {}/{ev}.log{extra}", dir.display());
            let matcher = if ev == "PreToolUse" { "AskUserQuestion" } else { "*" };
            hooks.insert(ev.into(), json!([{"matcher": matcher, "hooks": [{"type": "command", "command": command}]}]));
        }
        let path = dir.join("settings.json");
        std::fs::write(&path, json!({"hooks": hooks}).to_string()).unwrap();
        Hooks::load(&path).unwrap()
    }

    fn logged(dir: &Path, ev: &str) -> Vec<Value> {
        std::fs::read_to_string(dir.join(format!("{ev}.log")))
            .unwrap_or_default()
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect()
    }

    fn record(mode: &str) -> Record {
        Record {
            args: vec![], mode: mode.into(), session_id: "s1".into(), task: None,
            cwd: "/w".into(), env: BTreeMap::new(), settings: "s".into(), prompt: None,
            scenario: "t".into(), steps_done: 0, hooks: vec![], answers: vec![],
            inputs: vec![], exited: None,
        }
    }

    struct Out {
        code: i32,
        text: String,
        record: Record,
        dir: tempfile::TempDir,
    }

    fn play(mode: &str, steps: &str, input: &str, answer: Option<&str>) -> Out {
        let dir = tempfile::tempdir().unwrap();
        let fake = tempfile::tempdir().unwrap();
        if let Some(a) = answer {
            std::fs::write(dir.path().join("answer"), a).unwrap();
        }
        let hooks = settings(dir.path());
        let scenario = Scenario::parse(&format!(r#"{{"steps":{steps}}}"#)).unwrap();
        let mut rec = record(mode);
        let mut out = Vec::new();
        let code = run(&mut rec, &scenario, &hooks, Some(fake.path()), &mut Cursor::new(input.as_bytes().to_vec()), &mut out);
        let saved = Record::load(fake.path(), "s1").unwrap();
        assert_eq!(saved, rec, "record saved");
        Out { code, text: String::from_utf8(out).unwrap(), record: rec, dir }
    }

    fn events(o: &Out) -> Vec<&str> {
        o.record.hooks.iter().map(|h| h.event.as_str()).collect()
    }

    #[test]
    fn session_start_first_with_source() {
        let o = play("new", "[]", "", None);
        assert_eq!(events(&o), ["SessionStart", "SessionEnd"]);
        assert_eq!(logged(o.dir.path(), "SessionStart")[0]["source"], "startup");
        let o = play("resume", "[]", "", None);
        assert_eq!(logged(o.dir.path(), "SessionStart")[0]["source"], "resume");
    }

    #[test]
    fn print_writes_text() {
        let o = play("new", r#"[{"print":"hello"}]"#, "", None);
        assert_eq!(o.text, "hello\n");
        assert_eq!(o.record.steps_done, 1);
    }

    #[test]
    fn trust_dialog_is_recognised_and_enter_continues() {
        let o = play("new", r#"[{"trust_dialog":true},{"print":"in"}]"#, "\n", None);
        let want = "Do you trust the files in this folder?\n/w\n❯ 1. Yes, proceed\n  2. No, exit\nin\n";
        assert_eq!(o.text, want);
        assert_eq!(trust_dialog_selection(o.text.as_bytes()), Some(true));
        assert_eq!(o.code, 0);
    }

    #[test]
    fn trust_dialog_no_exits_1() {
        let o = play("new", r#"[{"trust_dialog":true},{"print":"in"}]"#, "2\n", None);
        assert_eq!(o.code, 1);
        assert!(!o.text.contains("in\n"));
        assert_eq!(o.record.exited, Some(1));
        assert_eq!(events(&o).last(), Some(&"SessionEnd"));
    }

    #[test]
    fn ask_answered_by_hook() {
        let ans = r#"{"hookSpecificOutput":{"updatedInput":{"answers":{"q":"A"}}}}"#;
        let o = play("new", r#"[{"ask":{"question":"q","header":"h","options":["A","B"]}}]"#, "", Some(ans));
        assert_eq!(o.record.answers, vec![json!({"q": "A"})]);
        assert_eq!(events(&o), ["SessionStart", "PreToolUse", "PostToolUse", "SessionEnd"]);
        let pre = &logged(o.dir.path(), "PreToolUse")[0];
        assert_eq!(pre["tool_input"]["questions"][0]["question"], "q");
    }

    #[test]
    fn ask_released_reads_a_line() {
        let o = play("new", r#"[{"ask":{"question":"q","header":"h","options":["A","B"]}}]"#, "B\n", None);
        assert_eq!(o.record.answers, vec![Value::Null]);
        assert_eq!(o.record.inputs, vec!["B"]);
        assert!(o.text.contains("q"));
        assert_eq!(events(&o), ["SessionStart", "PreToolUse", "PostToolUse", "SessionEnd"]);
    }

    #[test]
    fn stop_runs_stop_hook() {
        let o = play("new", r#"[{"stop":"done"}]"#, "", None);
        assert_eq!(logged(o.dir.path(), "Stop")[0]["last_assistant_message"], "done");
    }

    #[test]
    fn wait_input_records_and_submits() {
        let o = play("new", r#"[{"wait_input":true}]"#, "do it\n", None);
        assert_eq!(o.record.inputs, vec!["do it"]);
        assert_eq!(logged(o.dir.path(), "UserPromptSubmit")[0]["prompt"], "do it");
    }

    #[test]
    fn fail_runs_stop_failure_and_exits_1() {
        let o = play("new", r#"[{"fail":"boom"},{"print":"never"}]"#, "", None);
        assert_eq!(o.code, 1);
        assert_eq!(logged(o.dir.path(), "StopFailure")[0]["error"], "boom");
        assert!(!o.text.contains("never"));
    }

    #[test]
    fn exit_runs_session_end_with_code() {
        let o = play("new", r#"[{"exit":3}]"#, "", None);
        assert_eq!(o.code, 3);
        assert_eq!(o.record.exited, Some(3));
        assert_eq!(logged(o.dir.path(), "SessionEnd").len(), 1);
    }

    #[test]
    fn hook_step_runs_event_with_minimal_payload() {
        let o = play("new", r#"[{"hook":"Notification"}]"#, "", None);
        let p = &logged(o.dir.path(), "Notification")[0];
        assert_eq!(p["session_id"], "s1");
        assert_eq!(p["hook_event_name"], "Notification");
    }

    #[test]
    fn sleep_step_runs() {
        let o = play("new", r#"[{"sleep_ms":5}]"#, "", None);
        assert_eq!(o.record.steps_done, 1);
    }

    #[test]
    fn after_last_step_each_line_is_answered_until_eof() {
        let o = play("new", "[]", "one\ntwo\n", None);
        assert_eq!(o.record.inputs, vec!["one", "two"]);
        assert_eq!(logged(o.dir.path(), "UserPromptSubmit").len(), 2);
        assert_eq!(logged(o.dir.path(), "Stop").len(), 2);
        assert_eq!(o.code, 0);
        assert_eq!(o.record.exited, Some(0));
        assert_eq!(events(&o).last(), Some(&"SessionEnd"));
    }

    #[test]
    fn eof_during_wait_input_ends_the_session() {
        let o = play("new", r#"[{"wait_input":true},{"print":"x"}]"#, "", None);
        assert_eq!(o.code, 0);
        assert_eq!(o.text, "");
        assert_eq!(events(&o), ["SessionStart", "SessionEnd"]);
    }
}
