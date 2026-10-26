//! The step runner of a fake session: plays a [`Scenario`] on a terminal (any
//! `BufRead` / `Write`), reporting through the settings' hooks and keeping the
//! [`Record`] up to date.

use std::io::{BufRead, Write};
use std::path::Path;
use std::time::Duration;

use serde_json::{Value, json};

use super::hooks::{HookRun, Hooks, payload};
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
    /// The last line ended with `\r`: a `\n` right after it belongs to that line.
    after_cr: bool,
}

/// How a step ended: go on, or leave with an exit code.
enum Flow {
    Next,
    /// Stop playing steps and wait for input, like Claude after a failed turn.
    Idle,
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
            self.record.hooks.push(HookResult {
                event: event.to_string(),
                exit_code: r.exit_code,
            });
        }
        runs
    }

    fn print(&mut self, text: &str) {
        let _ = self.out.write_all(text.as_bytes());
        let _ = self.out.flush();
    }

    /// One line from the terminal, ended by `\n` or a bare `\r` (a terminal in raw
    /// mode sends Enter as `\r`); `None` on EOF or a read error (hang-up).
    fn read_line(&mut self) -> Option<String> {
        let mut line = Vec::new();
        let mut byte = [0u8; 1];
        loop {
            let after_cr = std::mem::take(&mut self.after_cr);
            match self.input.read(&mut byte) {
                Ok(0) | Err(_) if line.is_empty() => return None,
                Ok(0) | Err(_) => break,
                // `\r\n` is one line end. No peeking after the `\r`: on a raw tty that
                // would hold the line back until the next key.
                Ok(_) if byte[0] == b'\n' && after_cr && line.is_empty() => continue,
                Ok(_) if byte[0] == b'\n' => break,
                Ok(_) if byte[0] == b'\r' => {
                    self.after_cr = true;
                    break;
                }
                Ok(_) => line.push(byte[0]),
            }
        }
        Some(String::from_utf8_lossy(&line).into_owned())
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
            Step::Stop(stop) => {
                let p = payload::stop_with(&session, &cwd, stop.message(), stop.background());
                self.hook("Stop", &p);
            }
            Step::Write { path, content } => {
                if let Err(e) = write_file(&cwd, path, content) {
                    eprintln!("fake-claude: {e}");
                    return Flow::Exit(self.end("error", 2));
                }
            }
            Step::WaitInput(_) => match self.read_line() {
                None => return Flow::Exit(self.end("hangup", 0)),
                Some(line) => {
                    self.hook(
                        "UserPromptSubmit",
                        &payload::user_prompt_submit(&session, &cwd, &line),
                    );
                    self.record.inputs.push(line);
                }
            },
            Step::Fail(message) => {
                self.print(&format!("{message}\n"));
                self.hook(
                    "StopFailure",
                    &payload::stop_failure(&session, &cwd, message),
                );
                return Flow::Idle;
            }
            Step::Exit(code) => return Flow::Exit(self.end("exit", *code)),
        }
        Flow::Next
    }

    fn ask(&mut self, ask: &Ask) -> Flow {
        let (session, cwd) = (self.record.session_id.clone(), self.record.cwd.clone());
        let options = ask.options.iter().map(|o| o.to_json()).collect();
        let pre =
            payload::pre_tool_use_ask_with(&session, &cwd, &ask.question, &ask.header, options);
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
                    text.push_str(&format!("  {}. {}\n", i + 1, o.label()));
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

/// Plays a `write` step: `path` is relative to `cwd`, with no `..`; parents are created
/// and an existing file is overwritten.
fn write_file(cwd: &str, path: &str, content: &str) -> Result<(), String> {
    use std::path::Component;
    let rel = Path::new(path);
    if path.is_empty()
        || rel.is_absolute()
        || rel
            .components()
            .any(|c| !matches!(c, Component::Normal(_) | Component::CurDir))
    {
        return Err(format!(
            "write: invalid path `{path}` (must be relative, no `..`)"
        ));
    }
    let full = Path::new(cwd).join(rel);
    if let Some(parent) = full.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("write {path}: {e}"))?;
    }
    std::fs::write(&full, content).map_err(|e| format!("write {path}: {e}"))
}

/// The Env's temp home, derived from the fake dir (`<root>/fake`, `<root>/home`).
fn env_home(fake_dir: &Path) -> Option<std::path::PathBuf> {
    fake_dir.parent().map(|root| root.join("home"))
}

/// Claude keeps a conversation at `$HOME/.claude/projects/<cwd, non-alphanumerics as
/// dashes>/<session>.jsonl`; the daemon passes `--resume` only if that file exists
/// (`claude::has_conversation`). Written only when the record's `HOME` is absolute and
/// is the Env's own temp home (`<fake_dir>/../home`), so the fake can never touch a
/// real home; otherwise nothing is written. Returns the file's path when it is there.
pub fn write_transcript(record: &Record, fake_dir: Option<&Path>) -> Option<std::path::PathBuf> {
    let (Some(home), Some(fake_dir)) = (record.env.get("HOME"), fake_dir) else {
        return None;
    };
    let home = Path::new(home);
    let id = &record.session_id;
    if !home.is_absolute()
        || id.is_empty()
        || id.contains('/')
        || id.contains('\\')
        || id.contains("..")
    {
        return None;
    }
    let (Ok(got), Some(want)) = (home.canonicalize(), env_home(fake_dir)) else {
        return None;
    };
    if want.canonicalize().ok().as_ref() != Some(&got) {
        return None;
    }
    let cwd = Path::new(&record.cwd);
    let dir = cwd.canonicalize().unwrap_or_else(|_| cwd.to_path_buf());
    let folder: String = dir
        .to_string_lossy()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    let dir = got.join(".claude/projects").join(folder);
    std::fs::create_dir_all(&dir).ok()?;
    let path = dir.join(format!("{id}.jsonl"));
    std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .ok()?;
    Some(path)
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
    let mut r = Runner {
        record,
        hooks,
        fake_dir,
        input,
        out,
        after_cr: false,
    };
    write_transcript(r.record, fake_dir);
    let mut steps = scenario.steps.iter();
    // Real Claude shows the folder-trust dialog before the session starts.
    if let Some(first @ Step::TrustDialog(_)) = scenario.steps.first() {
        steps.next();
        let flow = r.step(first);
        r.record.steps_done += 1;
        r.save();
        if let Flow::Exit(code) = flow {
            return code;
        }
    }
    let source = if r.record.mode == "resume" {
        "resume"
    } else {
        "startup"
    };
    let start = payload::session_start(&r.record.session_id, &r.record.cwd, source);
    r.hook("SessionStart", &start);
    r.save();

    for step in steps {
        let flow = r.step(step);
        r.record.steps_done += 1;
        r.save();
        match flow {
            Flow::Exit(code) => return code,
            Flow::Idle => break,
            Flow::Next => {}
        }
    }

    // Idle, like Claude waiting for a prompt: answer every line typed.
    while let Some(line) = r.read_line() {
        if line.trim().is_empty() {
            continue;
        }
        let (session, cwd) = (r.record.session_id.clone(), r.record.cwd.clone());
        r.record.inputs.push(line.clone());
        r.hook(
            "UserPromptSubmit",
            &payload::user_prompt_submit(&session, &cwd, &line),
        );
        r.hook(
            "Stop",
            &payload::stop(&session, &cwd, &format!("Echo: {line}")),
        );
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
        "SessionStart",
        "UserPromptSubmit",
        "PreToolUse",
        "PostToolUse",
        "Stop",
        "StopFailure",
        "Notification",
        "SessionEnd",
    ];

    /// Settings whose hooks append their stdin (one line each) to `<dir>/<Event>.log`;
    /// `PreToolUse` then prints the contents of `<dir>/answer`, if any.
    fn settings(dir: &Path) -> Hooks {
        let mut hooks = serde_json::Map::new();
        for ev in EVENTS {
            let extra = if ev == "PreToolUse" {
                format!("; cat {}/answer 2>/dev/null; true", dir.display())
            } else {
                String::new()
            };
            let command = format!("{{ cat; echo; }} >> {}/{ev}.log{extra}", dir.display());
            let matcher = if ev == "PreToolUse" {
                "AskUserQuestion"
            } else {
                "*"
            };
            hooks.insert(
                ev.into(),
                json!([{"matcher": matcher, "hooks": [{"type": "command", "command": command}]}]),
            );
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
            args: vec![],
            mode: mode.into(),
            session_id: "s1".into(),
            task: None,
            cwd: "/w".into(),
            env: BTreeMap::new(),
            settings: "s".into(),
            prompt: None,
            scenario: "t".into(),
            steps_done: 0,
            hooks: vec![],
            answers: vec![],
            inputs: vec![],
            permissions: vec![],
            exited: None,
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
        let code = run(
            &mut rec,
            &scenario,
            &hooks,
            Some(fake.path()),
            &mut Cursor::new(input.as_bytes().to_vec()),
            &mut out,
        );
        let saved = Record::load(fake.path(), "s1").unwrap();
        assert_eq!(saved, rec, "record saved");
        Out {
            code,
            text: String::from_utf8(out).unwrap(),
            record: rec,
            dir,
        }
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
    fn trust_dialog_comes_before_session_start() {
        let o = play(
            "new",
            r#"[{"trust_dialog":true},{"print":"in"}]"#,
            "\n",
            None,
        );
        assert_eq!(events(&o), ["SessionStart", "SessionEnd"]);
        assert!(o.text.starts_with("Do you trust"), "{}", o.text);
        assert_eq!(o.record.steps_done, 2);
        // Declining ends the session before it ever started.
        let o = play(
            "new",
            r#"[{"trust_dialog":true},{"print":"in"}]"#,
            "2\n",
            None,
        );
        assert_eq!(events(&o), ["SessionEnd"]);
    }

    #[test]
    fn writes_the_transcript_the_daemon_looks_for() {
        let (dir, root, rec) = transcript_run("s1", |home| home.display().to_string());
        let canon = dir.path().canonicalize().unwrap();
        let folder: String = canon
            .to_string_lossy()
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
            .collect();
        let want = root
            .path()
            .join("home/.claude/projects")
            .join(folder)
            .join("s1.jsonl");
        assert!(want.is_file());
        assert_eq!(rec.session_id, "s1");
    }

    /// Runs an empty scenario with fake dir `<root>/fake`; `home_of` maps the Env's
    /// home to the record's `HOME`.
    fn transcript_run(
        session: &str,
        home_of: impl Fn(&Path) -> String,
    ) -> (tempfile::TempDir, tempfile::TempDir, Record) {
        let dir = tempfile::tempdir().unwrap();
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(root.path().join("fake")).unwrap();
        std::fs::create_dir_all(root.path().join("home")).unwrap();
        let hooks = settings(dir.path());
        let mut rec = record("new");
        rec.session_id = session.into();
        rec.cwd = dir.path().display().to_string();
        rec.env
            .insert("HOME".into(), home_of(&root.path().join("home")));
        let fake = root.path().join("fake");
        run(
            &mut rec,
            &Scenario {
                steps: vec![],
                turns: vec![],
            },
            &hooks,
            Some(&fake),
            &mut Cursor::new(vec![]),
            &mut Vec::new(),
        );
        (dir, root, rec)
    }

    fn jsonl_count(home: &Path) -> usize {
        fn walk(p: &Path) -> usize {
            std::fs::read_dir(p)
                .map(|d| {
                    d.flatten()
                        .map(|e| {
                            let p = e.path();
                            if p.is_dir() {
                                walk(&p)
                            } else if p.extension().is_some_and(|x| x == "jsonl") {
                                1
                            } else {
                                0
                            }
                        })
                        .sum()
                })
                .unwrap_or(0)
        }
        walk(home)
    }

    #[test]
    fn no_transcript_for_a_home_that_is_not_the_envs() {
        let other = tempfile::tempdir().unwrap();
        let o = other.path().display().to_string();
        let (_d, root, _) = transcript_run("s1", |_| o.clone());
        assert_eq!(jsonl_count(other.path()), 0);
        assert_eq!(jsonl_count(&root.path().join("home")), 0);
        let (_d, root, _) = transcript_run("s1", |_| "/Users/someone".into());
        assert_eq!(jsonl_count(&root.path().join("home")), 0);
    }

    #[test]
    fn no_transcript_for_a_relative_home_or_a_bad_session_id() {
        let (_d, root, _) = transcript_run("s1", |_| "home".into());
        assert_eq!(jsonl_count(root.path()), 0);
        for bad in ["../x", "a/b", ".."] {
            let (_d, root, _) = transcript_run(bad, |h| h.display().to_string());
            assert_eq!(jsonl_count(root.path()), 0, "{bad}");
        }
    }

    #[test]
    fn print_writes_text() {
        let o = play("new", r#"[{"print":"hello"}]"#, "", None);
        assert_eq!(o.text, "hello\n");
        assert_eq!(o.record.steps_done, 1);
    }

    #[test]
    fn trust_dialog_is_recognised_and_enter_continues() {
        let o = play(
            "new",
            r#"[{"trust_dialog":true},{"print":"in"}]"#,
            "\n",
            None,
        );
        let want =
            "Do you trust the files in this folder?\n/w\n❯ 1. Yes, proceed\n  2. No, exit\nin\n";
        assert_eq!(o.text, want);
        assert_eq!(trust_dialog_selection(o.text.as_bytes()), Some(true));
        assert_eq!(o.code, 0);
    }

    #[test]
    fn bare_carriage_return_is_yes_and_crlf_is_one_line() {
        let o = play(
            "new",
            r#"[{"trust_dialog":true},{"print":"in"}]"#,
            "\r",
            None,
        );
        assert!(o.text.ends_with("in\n"), "{}", o.text);
        let o = play(
            "new",
            r#"[{"wait_input":true},{"wait_input":true}]"#,
            "a\r\nb\rc",
            None,
        );
        assert_eq!(o.record.inputs, vec!["a", "b", "c"]);
        let o = play(
            "new",
            r#"[{"trust_dialog":true},{"print":"in"}]"#,
            "2\r",
            None,
        );
        assert_eq!(o.code, 1);
    }

    #[test]
    fn ask_with_task_card_matches_the_daemon_proposal_shape() {
        let steps = r#"[{"ask":{"question":"Add search","header":"Nueva tarea","options":[
            {"label":"Crear","description":"create it","preview":"the brief"},
            "Descartar"]}}]"#;
        let o = play("new", steps, "x\n", None);
        let pre = &logged(o.dir.path(), "PreToolUse")[0];
        let q = &pre["tool_input"]["questions"][0];
        assert_eq!(q["header"], "Nueva tarea");
        assert_eq!(q["question"], "Add search");
        let crear = q["options"]
            .as_array()
            .unwrap()
            .iter()
            .find(|o| o["label"] == "Crear")
            .unwrap();
        assert_eq!(crear["preview"], "the brief");
        assert_eq!(crear["description"], "create it");
        assert_eq!(
            q["options"][1],
            json!({"label":"Descartar","description":""})
        );
    }

    #[test]
    fn record_is_saved_after_each_step() {
        // Each hook copies the record file as it is at that moment.
        let dir = tempfile::tempdir().unwrap();
        let fake = tempfile::tempdir().unwrap();
        let rec = fake.path().join("records/s1.json");
        let cmd = format!(
            "cat >/dev/null; cp {} {}/snap.$(date +%s%N)",
            rec.display(),
            dir.path().display()
        );
        let path = dir.path().join("settings.json");
        std::fs::write(
            &path,
            json!({"hooks": {"Stop": [{"matcher": "*", "hooks": [{"type": "command", "command": cmd}]}]}}).to_string(),
        )
        .unwrap();
        let hooks = Hooks::load(&path).unwrap();
        let scenario =
            Scenario::parse(r#"{"steps":[{"print":"a"},{"stop":"1"},{"stop":"2"}]}"#).unwrap();
        let mut r = record("new");
        run(
            &mut r,
            &scenario,
            &hooks,
            Some(fake.path()),
            &mut Cursor::new(Vec::new()),
            &mut Vec::new(),
        );
        let mut snaps: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().starts_with("snap."))
            .map(|e| e.path())
            .collect();
        snaps.sort();
        let steps: Vec<usize> = snaps
            .iter()
            .map(|p| {
                serde_json::from_str::<Record>(&std::fs::read_to_string(p).unwrap())
                    .unwrap()
                    .steps_done
            })
            .collect();
        // The first stop runs after step 1 was saved; the second after step 2.
        assert_eq!(steps, vec![1, 2]);
    }

    #[test]
    fn trust_dialog_no_exits_1() {
        let o = play(
            "new",
            r#"[{"trust_dialog":true},{"print":"in"}]"#,
            "2\n",
            None,
        );
        assert_eq!(o.code, 1);
        assert!(!o.text.contains("in\n"));
        assert_eq!(o.record.exited, Some(1));
        assert_eq!(events(&o).last(), Some(&"SessionEnd"));
    }

    #[test]
    fn ask_answered_by_hook() {
        let ans = r#"{"hookSpecificOutput":{"updatedInput":{"answers":{"q":"A"}}}}"#;
        let o = play(
            "new",
            r#"[{"ask":{"question":"q","header":"h","options":["A","B"]}}]"#,
            "",
            Some(ans),
        );
        assert_eq!(o.record.answers, vec![json!({"q": "A"})]);
        assert_eq!(
            events(&o),
            ["SessionStart", "PreToolUse", "PostToolUse", "SessionEnd"]
        );
        let pre = &logged(o.dir.path(), "PreToolUse")[0];
        assert_eq!(pre["tool_input"]["questions"][0]["question"], "q");
    }

    #[test]
    fn ask_released_reads_a_line() {
        let o = play(
            "new",
            r#"[{"ask":{"question":"q","header":"h","options":["A","B"]}}]"#,
            "B\n",
            None,
        );
        assert_eq!(o.record.answers, vec![Value::Null]);
        assert_eq!(o.record.inputs, vec!["B"]);
        assert!(o.text.contains("q"));
        assert_eq!(
            events(&o),
            ["SessionStart", "PreToolUse", "PostToolUse", "SessionEnd"]
        );
    }

    #[test]
    fn stop_runs_stop_hook() {
        let o = play("new", r#"[{"stop":"done"}]"#, "", None);
        assert_eq!(
            logged(o.dir.path(), "Stop")[0]["last_assistant_message"],
            "done"
        );
    }

    #[test]
    fn wait_input_records_and_submits() {
        let o = play("new", r#"[{"wait_input":true}]"#, "do it\n", None);
        assert_eq!(o.record.inputs, vec!["do it"]);
        assert_eq!(
            logged(o.dir.path(), "UserPromptSubmit")[0]["prompt"],
            "do it"
        );
    }

    #[test]
    fn fail_runs_stop_failure_and_stays_alive_until_eof() {
        let o = play(
            "new",
            r#"[{"fail":"boom"},{"print":"never"}]"#,
            "again\n",
            None,
        );
        assert_eq!(logged(o.dir.path(), "StopFailure")[0]["error"], "boom");
        assert!(!o.text.contains("never"));
        // Like Claude after a failed turn: still reading input, SessionEnd only at EOF.
        assert_eq!(o.record.inputs, vec!["again"]);
        assert_eq!(
            events(&o),
            [
                "SessionStart",
                "StopFailure",
                "UserPromptSubmit",
                "Stop",
                "SessionEnd"
            ]
        );
        assert_eq!(o.code, 0);
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

    #[test]
    fn stop_object_logs_background_tasks() {
        let steps = r#"[{"stop":{"message":"m","background":[{"id":"b1","status":"running"}]}},
            {"stop":{"message":"n","background":[]}},{"stop":"plain"}]"#;
        let o = play("new", steps, "", None);
        let stops = logged(o.dir.path(), "Stop");
        assert_eq!(stops[0]["last_assistant_message"], "m");
        assert_eq!(
            stops[0]["background_tasks"],
            json!([{"id":"b1","status":"running"}])
        );
        assert_eq!(stops[1]["background_tasks"], json!([]));
        assert!(stops[2].get("background_tasks").is_none());
    }

    fn play_in(cwd: &Path, steps: &str) -> Out {
        let dir = tempfile::tempdir().unwrap();
        let hooks = settings(dir.path());
        let scenario = Scenario::parse(&format!(r#"{{"steps":{steps}}}"#)).unwrap();
        let mut rec = record("new");
        rec.cwd = cwd.display().to_string();
        let mut out = Vec::new();
        let code = run(
            &mut rec,
            &scenario,
            &hooks,
            None,
            &mut Cursor::new(Vec::new()),
            &mut out,
        );
        Out {
            code,
            text: String::new(),
            record: rec,
            dir,
        }
    }

    #[test]
    fn write_creates_the_file_with_parents_and_overwrites() {
        let cwd = tempfile::tempdir().unwrap();
        let o = play_in(
            cwd.path(),
            r#"[{"write":{"path":"specs/k-1-x/tasks.md","content":"old"}},
            {"write":{"path":"specs/k-1-x/tasks.md","content":"- [ ] T1"}}]"#,
        );
        assert_eq!(o.code, 0);
        assert_eq!(
            std::fs::read_to_string(cwd.path().join("specs/k-1-x/tasks.md")).unwrap(),
            "- [ ] T1"
        );
        assert!(logged(o.dir.path(), "Stop").is_empty());
    }

    #[test]
    fn write_with_a_bad_path_exits_2() {
        for bad in ["/etc/x", "../x", "a/../../x", ""] {
            let cwd = tempfile::tempdir().unwrap();
            let steps =
                format!(r#"[{{"write":{{"path":"{bad}","content":"x"}}}},{{"print":"never"}}]"#);
            let o = play_in(cwd.path(), &steps);
            assert_eq!(o.code, 2, "{bad}");
            assert_eq!(o.record.exited, Some(2), "{bad}");
        }
    }
}
