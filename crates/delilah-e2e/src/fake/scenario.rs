//! Scripted behaviour of a fake session: `<fake_dir>/scenarios/<name>.json`.

use std::path::Path;

use serde::Deserialize;

/// The longest a `sleep_ms` step waits.
pub const MAX_SLEEP_MS: u64 = 5000;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Scenario {
    /// What a terminal (PTY) session plays.
    #[serde(default)]
    pub steps: Vec<Step>,
    /// What a chat-mode (stream-json) session plays: one list of steps per user turn, in
    /// order; once they run out, each turn answers `Echo: <message>`.
    #[serde(default)]
    pub turns: Vec<Vec<StreamStep>>,
}

/// A step of a chat-mode turn.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StreamStep {
    /// An assistant text message, streamed as word deltas `delay_ms` apart.
    Say(Say),
    /// A tool call: asks `can_use_tool` first (unless `ask` is false), then gives the
    /// tool result (`output` when allowed, the denial message when not).
    Tool(Tool),
    SleepMs(u64),
    /// The process exits with this code, without finishing the turn.
    Exit(i32),
    /// The process writes `message` to stderr and exits with `code` before saying
    /// anything on stdout (not even `system/init`): a CLI that rejects its flags.
    ExitWithStderr(ExitWithStderr),
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ExitWithStderr {
    #[serde(default = "one")]
    pub code: i32,
    pub message: String,
}

fn one() -> i32 {
    1
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Say {
    pub text: String,
    #[serde(default)]
    pub delay_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Tool {
    pub name: String,
    pub input: serde_json::Value,
    #[serde(default)]
    pub output: String,
    #[serde(default = "yes")]
    pub ask: bool,
    /// What Claude's auto mode does with this tool: `allow` runs it without asking,
    /// `ask` still sends `can_use_tool`. Semantics mirror
    /// `docs/archive/spikes/2026-10-26-chat-permission-mode.md`.
    #[serde(default)]
    pub auto: AutoMode,
    /// `permission_suggestions` of the request; left out: Claude's usual one, an
    /// `addRules` with the exact command (Bash) or the bare tool.
    #[serde(default)]
    pub suggestions: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AutoMode {
    #[default]
    Allow,
    Ask,
}

fn yes() -> bool {
    true
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Step {
    TrustDialog(bool),
    Print(String),
    Hook(String),
    SleepMs(u64),
    Ask(Ask),
    Stop(StopStep),
    /// Writes `content` to `path`, relative to the session's cwd.
    Write {
        path: String,
        content: String,
    },
    WaitInput(bool),
    Fail(String),
    Exit(i32),
    /// Writes `message` to the terminal (it is stderr, for a real CLI) and exits.
    ExitWithStderr(ExitWithStderr),
}

/// A `stop` step: the message alone, or the message plus the background tasks the
/// end-of-turn report lists (`[]` allowed; left out: no `background_tasks` field).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StopStep {
    Text(String),
    Full {
        message: String,
        background: Option<Vec<serde_json::Value>>,
    },
}

impl StopStep {
    pub fn message(&self) -> &str {
        match self {
            StopStep::Text(m) | StopStep::Full { message: m, .. } => m,
        }
    }

    pub fn background(&self) -> Option<&[serde_json::Value]> {
        match self {
            StopStep::Text(_) => None,
            StopStep::Full { background, .. } => background.as_deref(),
        }
    }
}

impl<'de> Deserialize<'de> for StopStep {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        use serde::de::Error;
        let v = serde_json::Value::deserialize(d)?;
        match v {
            serde_json::Value::String(s) => Ok(StopStep::Text(s)),
            serde_json::Value::Object(mut o) => {
                let message = match o.remove("message") {
                    Some(serde_json::Value::String(m)) => m,
                    _ => {
                        return Err(D::Error::custom(
                            "invalid `stop` step: the object needs a string `message`",
                        ));
                    }
                };
                let background = match o.remove("background") {
                    None | Some(serde_json::Value::Null) => None,
                    Some(serde_json::Value::Array(a)) => Some(a),
                    Some(_) => {
                        return Err(D::Error::custom(
                            "invalid `stop` step: `background` must be a list",
                        ));
                    }
                };
                Ok(StopStep::Full {
                    message,
                    background,
                })
            }
            _ => Err(D::Error::custom(
                "invalid `stop` step: expected a string or an object with `message`",
            )),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Ask {
    pub question: String,
    pub header: String,
    pub options: Vec<AskOption>,
}

/// An option of an `ask`: a plain label, or an object with the fields Claude's
/// `AskUserQuestion` accepts (`preview` carries the brief of a task proposal).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(untagged)]
pub enum AskOption {
    Plain(String),
    Full {
        label: String,
        #[serde(default)]
        description: Option<String>,
        #[serde(default)]
        preview: Option<String>,
    },
}

impl AskOption {
    pub fn label(&self) -> &str {
        match self {
            AskOption::Plain(l) | AskOption::Full { label: l, .. } => l,
        }
    }

    /// The option as it goes in `tool_input.questions[].options`.
    pub fn to_json(&self) -> serde_json::Value {
        match self {
            AskOption::Plain(l) => serde_json::json!({"label": l, "description": ""}),
            AskOption::Full {
                label,
                description,
                preview,
            } => {
                let mut v = serde_json::json!({
                    "label": label,
                    "description": description.as_deref().unwrap_or(""),
                });
                if let Some(p) = preview {
                    v["preview"] = serde_json::json!(p);
                }
                v
            }
        }
    }
}

impl From<&str> for AskOption {
    fn from(s: &str) -> Self {
        AskOption::Plain(s.to_string())
    }
}

impl Scenario {
    /// Parses a scenario; an unknown step is an error naming it. `sleep_ms`
    /// is capped at [`MAX_SLEEP_MS`].
    pub fn parse(json: &str) -> Result<Scenario, String> {
        let mut s: Scenario =
            serde_json::from_str(json).map_err(|e| format!("invalid scenario: {e}"))?;
        for step in &mut s.steps {
            if let Step::SleepMs(ms) = step {
                *ms = (*ms).min(MAX_SLEEP_MS);
            }
        }
        for step in s.turns.iter_mut().flatten() {
            match step {
                StreamStep::SleepMs(ms) => *ms = (*ms).min(MAX_SLEEP_MS),
                StreamStep::Say(say) => say.delay_ms = say.delay_ms.min(MAX_SLEEP_MS),
                _ => {}
            }
        }
        Ok(s)
    }
}

/// The `<name>` of a `[fake:<name>]` marker in the prompt.
fn marker(prompt: &str) -> Option<&str> {
    let rest = &prompt[prompt.find("[fake:")? + "[fake:".len()..];
    let name = &rest[..rest.find(']')?];
    (!name.is_empty()).then_some(name)
}

/// A scenario name is a plain file stem: `[A-Za-z0-9._-]+`, no leading dot. One
/// leading `@` is allowed: a project conversation's task is `@<project name>`.
pub fn valid_name(name: &str) -> bool {
    let name = name.strip_prefix('@').unwrap_or(name);
    !name.is_empty()
        && !name.starts_with('.')
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
}

/// The `<command>` of a prompt that starts with `/<command>`, without the slash.
fn command(prompt: &str) -> Option<&str> {
    let rest = prompt.trim_start().strip_prefix('/')?;
    let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
    let word = &rest[..end];
    (!word.is_empty()).then_some(word)
}

/// Picks the scenario: `[fake:<name>]` in the prompt, else `<task>.<command>.json`
/// (`<command>` is the prompt's leading `/word`), else `<task>.json`, else `default.json`. Returns the name chosen and the scenario.
pub fn select(
    fake_dir: &Path,
    prompt: Option<&str>,
    task: Option<&str>,
) -> Result<(String, Scenario), String> {
    let dir = fake_dir.join("scenarios");
    let load = |name: &str| -> Result<Option<Scenario>, String> {
        if !valid_name(name) {
            return Err(format!("invalid scenario name `{name}`"));
        }
        let path = dir.join(format!("{name}.json"));
        match std::fs::read_to_string(&path) {
            Ok(text) => Scenario::parse(&text)
                .map(Some)
                .map_err(|e| format!("{}: {e}", path.display())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(format!("{}: {e}", path.display())),
        }
    };
    if let Some(name) = prompt.and_then(marker) {
        let s = load(name)?
            .ok_or_else(|| format!("scenario `{name}` not found in {}", dir.display()))?;
        return Ok((name.to_string(), s));
    }
    if let Some(task) = task
        && let Some(cmd) = prompt.and_then(command)
    {
        let name = format!("{task}.{cmd}");
        if valid_name(&name)
            && let Some(s) = load(&name)?
        {
            return Ok((name, s));
        }
    }
    if let Some(task) = task
        && let Some(s) = load(task)?
    {
        return Ok((task.to_string(), s));
    }
    let s = load("default")?.ok_or_else(|| format!("no default.json in {}", dir.display()))?;
    Ok(("default".to_string(), s))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_leading_at_is_allowed_once_and_nothing_else_escapes() {
        assert!(valid_name("@signal"));
        assert!(valid_name("echo"));
        for bad in [
            "@../x", "@/x", "@.x", "@", "@@x", "a/b", "a\\b", "..", ".x", "",
        ] {
            assert!(!valid_name(bad), "{bad}");
        }
    }

    #[test]
    fn parses_every_step() {
        let s = Scenario::parse(
            r#"{"steps":[{"trust_dialog":true},{"print":"hi"},{"hook":"UserPromptSubmit"},
            {"sleep_ms":200},{"ask":{"question":"q","header":"h","options":["a",{"label":"c","description":"d","preview":"p"}]}},
            {"stop":"done"},{"wait_input":true},{"fail":"boom"},{"exit":3}]}"#,
        )
        .unwrap();
        assert_eq!(
            s.steps,
            vec![
                Step::TrustDialog(true),
                Step::Print("hi".into()),
                Step::Hook("UserPromptSubmit".into()),
                Step::SleepMs(200),
                Step::Ask(Ask {
                    question: "q".into(),
                    header: "h".into(),
                    options: vec![
                        "a".into(),
                        AskOption::Full {
                            label: "c".into(),
                            description: Some("d".into()),
                            preview: Some("p".into())
                        },
                    ]
                }),
                Step::Stop(StopStep::Text("done".into())),
                Step::WaitInput(true),
                Step::Fail("boom".into()),
                Step::Exit(3),
            ]
        );
    }

    #[test]
    fn parses_stream_turns() {
        let s = Scenario::parse(
            r#"{"turns":[[{"say":{"text":"hi","delay_ms":99999}},
            {"tool":{"name":"Bash","input":{"command":"ls"},"output":"a"}}],[{"exit":0}]]}"#,
        )
        .unwrap();
        assert!(s.steps.is_empty());
        assert_eq!(s.turns.len(), 2);
        assert_eq!(
            s.turns[0][0],
            StreamStep::Say(Say {
                text: "hi".into(),
                delay_ms: MAX_SLEEP_MS
            })
        );
        assert!(matches!(&s.turns[0][1], StreamStep::Tool(t) if t.ask && t.suggestions.is_none()));
        assert_eq!(s.turns[1], vec![StreamStep::Exit(0)]);
    }

    #[test]
    fn a_tool_auto_defaults_to_allow_and_parses_ask() {
        let s = Scenario::parse(
            r#"{"turns":[[{"tool":{"name":"A","input":{}}},{"tool":{"name":"B","input":{},"auto":"ask"}}]]}"#,
        )
        .unwrap();
        assert!(matches!(&s.turns[0][0], StreamStep::Tool(t) if t.auto == AutoMode::Allow));
        assert!(matches!(&s.turns[0][1], StreamStep::Tool(t) if t.auto == AutoMode::Ask));
        assert!(
            Scenario::parse(r#"{"turns":[[{"tool":{"name":"A","input":{},"auto":"x"}}]]}"#)
                .is_err()
        );
    }

    #[test]
    fn sleep_is_capped() {
        let s = Scenario::parse(r#"{"steps":[{"sleep_ms":999999}]}"#).unwrap();
        assert_eq!(s.steps, vec![Step::SleepMs(5000)]);
    }

    #[test]
    fn unknown_step_is_an_error_naming_it() {
        let e = Scenario::parse(r#"{"steps":[{"dance":1}]}"#).unwrap_err();
        assert!(e.contains("dance"), "{e}");
    }

    fn dir(files: &[(&str, &str)]) -> tempfile::TempDir {
        let d = tempfile::tempdir().unwrap();
        std::fs::create_dir(d.path().join("scenarios")).unwrap();
        for (name, body) in files {
            std::fs::write(
                d.path().join("scenarios").join(format!("{name}.json")),
                body,
            )
            .unwrap();
        }
        d
    }

    const A: &str = r#"{"steps":[{"print":"a"}]}"#;
    const B: &str = r#"{"steps":[{"print":"b"}]}"#;
    const C: &str = r#"{"steps":[{"print":"c"}]}"#;

    #[test]
    fn marker_beats_task_beats_default() {
        let d = dir(&[("default", A), ("T-1", B), ("pick", C)]);
        let (n, s) = select(d.path(), Some("/feature [fake:pick] T-1"), Some("T-1")).unwrap();
        assert_eq!(
            (n.as_str(), s.steps),
            ("pick", vec![Step::Print("c".into())])
        );
        let (n, _) = select(d.path(), Some("/feature T-1"), Some("T-1")).unwrap();
        assert_eq!(n, "T-1");
        let (n, _) = select(d.path(), Some("x"), Some("T-9")).unwrap();
        assert_eq!(n, "default");
    }

    #[test]
    fn marker_naming_a_missing_file_is_an_error() {
        let d = dir(&[("default", A)]);
        let e = select(d.path(), Some("[fake:nope]"), None).unwrap_err();
        assert!(e.contains("nope"), "{e}");
    }

    #[test]
    fn no_prompt_and_no_task_uses_default() {
        let d = dir(&[("default", A)]);
        let (n, _) = select(d.path(), None, None).unwrap();
        assert_eq!(n, "default");
    }

    #[test]
    fn unsafe_names_are_rejected() {
        let d = dir(&[("default", A)]);
        for bad in ["../x", "/etc/x", ".hidden", "a/b"] {
            let e = select(d.path(), Some(&format!("[fake:{bad}]")), None).unwrap_err();
            assert!(e.contains("invalid scenario name"), "{bad}: {e}");
            let e = select(d.path(), None, Some(bad)).unwrap_err();
            assert!(e.contains("invalid scenario name"), "{bad}: {e}");
        }
        select(d.path(), Some("[fake:default]"), Some("PROJ-1.x_y")).unwrap();
    }

    #[test]
    fn stop_object_form_and_write_step() {
        let s = Scenario::parse(
            r#"{"steps":[{"stop":"a"},{"stop":{"message":"b"}},
            {"stop":{"message":"c","background":[{"id":"x"}]}},{"stop":{"message":"d","background":[]}},
            {"write":{"path":"specs/k/tasks.md","content":"hi"}}]}"#,
        )
        .unwrap();
        assert_eq!(
            s.steps,
            vec![
                Step::Stop(StopStep::Text("a".into())),
                Step::Stop(StopStep::Full {
                    message: "b".into(),
                    background: None
                }),
                Step::Stop(StopStep::Full {
                    message: "c".into(),
                    background: Some(vec![serde_json::json!({"id":"x"})])
                }),
                Step::Stop(StopStep::Full {
                    message: "d".into(),
                    background: Some(vec![])
                }),
                Step::Write {
                    path: "specs/k/tasks.md".into(),
                    content: "hi".into()
                },
            ]
        );
    }

    #[test]
    fn malformed_stop_is_an_error_naming_the_step() {
        for bad in [r#"{"stop":3}"#, r#"{"stop":{"background":[]}}"#] {
            let e = Scenario::parse(&format!(r#"{{"steps":[{bad}]}}"#)).unwrap_err();
            assert!(e.contains("stop"), "{e}");
        }
    }

    #[test]
    fn command_scenario_beats_task_and_falls_back() {
        let d = dir(&[("default", A), ("K-1", B), ("K-1.feature", C)]);
        let (n, s) = select(d.path(), Some("/feature K-1 do it"), Some("K-1")).unwrap();
        assert_eq!(
            (n.as_str(), s.steps),
            ("K-1.feature", vec![Step::Print("c".into())])
        );
        let (n, _) = select(d.path(), Some("/orchestrate specs/x"), Some("K-1")).unwrap();
        assert_eq!(n, "K-1");
        let (n, _) = select(d.path(), Some("no command"), Some("K-1")).unwrap();
        assert_eq!(n, "K-1");
        let (n, _) = select(d.path(), Some("/feature"), Some("K-1")).unwrap();
        assert_eq!(n, "K-1.feature");
    }
}
