//! Scripted behaviour of a fake session: `<fake_dir>/scenarios/<name>.json`.

use std::path::Path;

use serde::Deserialize;

/// The longest a `sleep_ms` step waits.
pub const MAX_SLEEP_MS: u64 = 5000;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Scenario {
    pub steps: Vec<Step>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Step {
    TrustDialog(bool),
    Print(String),
    Hook(String),
    SleepMs(u64),
    Ask(Ask),
    Stop(String),
    WaitInput(bool),
    Fail(String),
    Exit(i32),
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

/// Picks the scenario: `[fake:<name>]` in the prompt, else `<task>.json`,
/// else `default.json`. Returns the name chosen and the scenario.
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
                Step::Stop("done".into()),
                Step::WaitInput(true),
                Step::Fail("boom".into()),
                Step::Exit(3),
            ]
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
}
