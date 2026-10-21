//! Claude Code adapter. Findings behind these choices: `docs/spikes/2026-10-19-claude-pty.md`.

use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use portable_pty::CommandBuilder;
use serde::Serialize;
use serde_json::{Value, json};

/// Hook events Delilah listens to.
/// How long AskUserQuestion may wait for an answer from Delilah: a day.
pub const ASK_TIMEOUT_SECS: u64 = 86_400;

const HOOK_EVENTS: &[&str] = &[
    "SessionStart",
    "UserPromptSubmit",
    "PostToolUse",
    "PostToolUseFailure",
    "SubagentStart",
    "SubagentStop",
    "Notification",
    "Stop",
    "StopFailure",
    "SessionEnd",
];

/// Variables passed through to the agent. Everything else is dropped: launched with the
/// environment of another Claude Code session, Claude does not save its transcript.
pub(crate) const KEPT_ENV: &[&str] = &[
    "HOME",
    "USER",
    "LOGNAME",
    "SHELL",
    "PATH",
    "LANG",
    "LC_ALL",
    "TMPDIR",
    // Corporate networks: proxies and their CA certificates.
    "HTTPS_PROXY",
    "HTTP_PROXY",
    "NO_PROXY",
    "ALL_PROXY",
    "https_proxy",
    "http_proxy",
    "no_proxy",
    "all_proxy",
    "SSL_CERT_FILE",
    "NODE_EXTRA_CA_CERTS",
];

pub struct Launch<'a> {
    pub workspace: &'a Path,
    pub session_id: &'a str,
    pub settings: &'a Path,
    /// First prompt, sent as soon as the session starts (also when resuming).
    pub prompt: Option<&'a str>,
    pub resume: bool,
    /// Folders outside the workspace the agent may read, e.g. the task's ticket.
    pub extra_dirs: &'a [&'a Path],
    /// Plan mode: Claude reads and explains, it does not edit.
    pub read_only: bool,
    /// Remote Control: the session can also be followed and driven from claude.ai
    /// and the Claude app, under this name.
    pub remote_control: Option<&'a str>,
}

/// Whether Claude saved a conversation for `session_id` started in `workspace`, i.e.
/// whether `--resume` will find it: a session closed before its first message has none.
pub fn has_conversation(workspace: &Path, session_id: &str) -> bool {
    let Some(home) = std::env::var_os("HOME") else {
        return false;
    };
    let dir = workspace
        .canonicalize()
        .unwrap_or_else(|_| workspace.to_path_buf());
    // Claude names a project's folder after its path, every other character a dash.
    let folder: String = dir
        .to_string_lossy()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    Path::new(&home)
        .join(".claude/projects")
        .join(folder)
        .join(format!("{session_id}.jsonl"))
        .is_file()
}

pub fn command(launch: &Launch) -> CommandBuilder {
    let mut cmd = CommandBuilder::new("claude");
    cmd.env_clear();
    for (key, value) in crate::env::agent_env() {
        cmd.env(key, value);
    }
    cmd.env("TERM", "xterm-256color");
    cmd.env("COLORTERM", "truecolor");

    if launch.resume {
        cmd.args(["--resume", launch.session_id]);
    } else {
        cmd.args(["--session-id", launch.session_id]);
    }
    // `--add-dir` takes several values: keep another option after it so it never
    // swallows the prompt.
    for dir in launch.extra_dirs {
        cmd.arg("--add-dir");
        cmd.arg(dir);
    }
    // The name is always given: `--remote-control` alone would take the prompt as one.
    if let Some(name) = launch.remote_control {
        cmd.args(["--remote-control", name]);
    }
    cmd.arg("--settings");
    cmd.arg(launch.settings);
    if launch.read_only {
        cmd.args(["--permission-mode", "plan"]);
    }
    if let Some(prompt) = launch.prompt {
        cmd.arg(prompt);
    }
    cmd.cwd(launch.workspace);
    cmd
}

/// Settings for one session: every hook posts its JSON to the daemon, and the status
/// line runs `status_script` (see [`status_script`]). Lives outside the repo and is
/// passed with `--settings`.
pub fn settings(
    daemon_url: &str,
    task: &str,
    session: &str,
    status_script: &Path,
    padding: Option<&Value>,
) -> Value {
    let post = post_command(daemon_url, "/hooks/claude", task, session);
    let mut hooks: serde_json::Map<String, Value> = HOOK_EVENTS
        .iter()
        .map(|event| {
            let entry =
                json!([{ "matcher": "*", "hooks": [{ "type": "command", "command": post }] }]);
            (event.to_string(), entry)
        })
        .collect();
    // AskUserQuestion waits here for an answer given in Delilah (see the daemon's
    // asks.rs); if the daemon is away, curl prints nothing and the terminal asks.
    let ask = format!(
        "curl -sS -m 86400 -X POST -H 'content-type: application/json' --data-binary @- \
         '{daemon_url}/hooks/claude/ask?task={task}&session={session}' 2>/dev/null || true"
    );
    hooks.insert(
        "PreToolUse".into(),
        json!([
            {
                "matcher": "AskUserQuestion",
                "hooks": [{ "type": "command", "command": ask, "timeout": ASK_TIMEOUT_SECS }]
            },
            // Every other tool start feeds the activity view; the answer is not awaited.
            { "matcher": "*", "hooks": [{ "type": "command", "command": post }] }
        ]),
    );
    let mut status_line =
        json!({ "type": "command", "command": format!("sh '{}'", status_script.display()) });
    if let Some(padding) = padding {
        status_line["padding"] = padding.clone();
    }
    json!({ "hooks": hooks, "statusLine": status_line })
}

fn post_command(daemon_url: &str, path: &str, task: &str, session: &str) -> String {
    format!(
        "curl -sS -m 3 -X POST -H 'content-type: application/json' --data-binary @- \
         '{daemon_url}{path}?task={task}&session={session}' >/dev/null 2>&1 || true"
    )
}

/// The status line the user configured for Claude in `~/.claude/settings.json`.
/// `--settings` would otherwise replace it, so Delilah runs it itself. Only the
/// user's own: a repo's `.claude/settings*.json` is not trusted to run commands,
/// and Delilah would run it without Claude's folder trust prompt.
pub fn user_status_line() -> Option<Value> {
    let path = PathBuf::from(std::env::var_os("HOME")?).join(".claude/settings.json");
    let settings: Value = serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()?;
    let line = settings.get("statusLine")?;
    (line["type"] == "command" && line["command"].is_string()).then(|| line.clone())
}

/// The status line script: forwards Claude's status JSON (with the usage windows) to
/// the daemon, then draws the user's own status line from the same JSON, or a plain
/// "Delilah · KEY" if they have none.
pub fn status_script(
    daemon_url: &str,
    task: &str,
    session: &str,
    user_command: Option<&str>,
) -> String {
    let forward = post_command(daemon_url, "/hooks/claude/statusline", task, session);
    let draw = match user_command {
        // The user's command goes in verbatim, as the body of a function: no quoting to break.
        Some(cmd) => format!(
            "user_status_line() {{\n{cmd}\n}}\nprintf '%s' \"$payload\" | user_status_line\n"
        ),
        None => format!("echo 'Delilah · {task}'\n"),
    };
    format!(
        "#!/bin/sh\n# Written by Delilah for one session.\npayload=$(cat)\nprintf '%s' \"$payload\" | {forward} &\n{draw}"
    )
}

pub fn write_settings(
    path: &Path,
    daemon_url: &str,
    task: &str,
    session: &str,
) -> std::io::Result<PathBuf> {
    let dir = path.parent().unwrap_or(Path::new("."));
    std::fs::create_dir_all(dir)?;
    let user = user_status_line();
    let script = dir.join(format!("statusline-{session}.sh"));
    std::fs::write(
        &script,
        status_script(
            daemon_url,
            task,
            session,
            user.as_ref().and_then(|u| u["command"].as_str()),
        ),
    )?;
    let settings = settings(
        daemon_url,
        task,
        session,
        &script,
        user.as_ref().and_then(|u| u.get("padding")),
    );
    std::fs::write(path, serde_json::to_vec_pretty(&settings)?)?;
    Ok(path.to_path_buf())
}

/// What a hook payload means for the task.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Signal {
    SessionStarted,
    /// The user (or Delilah) sent a prompt: the agent is working again.
    PromptSubmitted,
    /// A tool just ran: the agent is working (e.g. after the user approved it).
    Working,
    /// The agent is blocked on the user: a permission prompt or an idle notification.
    NeedsUser,
    /// The turn ended normally and the agent waits for the next message.
    TurnEnded,
    /// The turn ended on an API error, such as a usage limit.
    TurnFailed,
    SessionEnded,
    Other,
}

pub fn classify(hook: &Value) -> Signal {
    match hook["hook_event_name"].as_str() {
        Some("SessionStart") => Signal::SessionStarted,
        Some("UserPromptSubmit") => Signal::PromptSubmitted,
        Some("PostToolUse") => Signal::Working,
        Some("Notification") => Signal::NeedsUser,
        Some("Stop") => Signal::TurnEnded,
        Some("StopFailure") => Signal::TurnFailed,
        Some("SessionEnd") => Signal::SessionEnded,
        _ => Signal::Other,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ActionKind {
    Edit,
    Read,
    Run,
    Delegate,
    Other,
}

/// A tool call in flight, as shown in the activity view.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Action {
    /// The `tool_use_id`.
    pub id: String,
    pub kind: ActionKind,
    pub tool: String,
    /// What the tool acts on, at most 40 chars. Never file contents.
    pub target: Option<String>,
    pub background: bool,
    pub since: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AgentRef {
    pub id: String,
    /// The `agent_type`, empty when missing.
    pub role: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum ActivityEvent {
    ToolStarted {
        agent: Option<AgentRef>,
        action: Action,
    },
    ToolEnded {
        agent: Option<AgentRef>,
        id: String,
    },
    HelperStarted {
        agent: AgentRef,
    },
    HelperEnded {
        id: String,
    },
    TurnOver,
}

const TARGET_MAX: usize = 40;

pub fn shorten(s: &str) -> String {
    if s.chars().count() <= TARGET_MAX {
        return s.to_string();
    }
    let mut out: String = s.chars().take(TARGET_MAX - 1).collect();
    out.push('…');
    out
}

/// The action a tool call performs. Only a short, safe target is kept.
pub fn action_of(tool: &str, input: &Value, id: &str) -> Action {
    let text = |key: &str| input[key].as_str().filter(|s| !s.is_empty());
    let file = |key: &str| {
        text(key).and_then(|p| {
            Path::new(p)
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
        })
    };
    let (kind, target) = match tool {
        "Edit" | "MultiEdit" | "Write" => (ActionKind::Edit, file("file_path")),
        "NotebookEdit" => (ActionKind::Edit, file("notebook_path")),
        "Read" => (ActionKind::Read, file("file_path")),
        "NotebookRead" => (ActionKind::Read, file("notebook_path")),
        "Grep" | "Glob" => (ActionKind::Read, text("pattern").map(String::from)),
        "LS" => (ActionKind::Read, None),
        "WebFetch" => {
            let host = text("url").and_then(|u| {
                let rest = u.split_once("://").map_or(u, |(_, r)| r);
                let host = rest.split(['/', '?', '#']).next().unwrap_or("");
                // Never keep credentials from a `user:pass@host` URL.
                let host = host.rsplit_once('@').map_or(host, |(_, h)| h);
                (!host.is_empty()).then(|| host.to_string())
            });
            (ActionKind::Read, host)
        }
        "WebSearch" => (ActionKind::Read, text("query").map(String::from)),
        "Bash" | "BashOutput" | "KillShell" => {
            let line = text("command")
                .and_then(|c| c.trim().lines().next())
                .map(|l| l.trim().to_string())
                .filter(|l| !l.is_empty());
            (ActionKind::Run, line)
        }
        "Agent" | "Task" => (
            ActionKind::Delegate,
            text("description")
                .or_else(|| text("subagent_type"))
                .map(String::from),
        ),
        _ => (ActionKind::Other, None),
    };
    Action {
        id: id.to_string(),
        kind,
        tool: tool.to_string(),
        target: target.map(|t| shorten(&t)),
        background: kind == ActionKind::Delegate
            && input["run_in_background"].as_bool().unwrap_or(false),
        since: Utc::now(),
    }
}

/// What a hook payload means for the activity view, if anything.
pub fn classify_activity(hook: &Value) -> Option<ActivityEvent> {
    let agent_id = hook["agent_id"].as_str().filter(|s| !s.is_empty());
    let agent = agent_id.map(|id| AgentRef {
        id: id.to_string(),
        role: hook["agent_type"].as_str().unwrap_or("").to_string(),
    });
    let tool_use_id = hook["tool_use_id"].as_str().unwrap_or("").to_string();
    // A waiting session must not show a stale AskUserQuestion action.
    if hook["tool_name"].as_str() == Some("AskUserQuestion") {
        return None;
    }
    match hook["hook_event_name"].as_str()? {
        "PreToolUse" => Some(ActivityEvent::ToolStarted {
            agent,
            action: action_of(
                hook["tool_name"].as_str().unwrap_or(""),
                &hook["tool_input"],
                &tool_use_id,
            ),
        }),
        "PostToolUse" | "PostToolUseFailure" => Some(ActivityEvent::ToolEnded {
            agent,
            id: tool_use_id,
        }),
        "SubagentStart" => agent.map(|agent| ActivityEvent::HelperStarted { agent }),
        "SubagentStop" => agent.map(|a| ActivityEvent::HelperEnded { id: a.id }),
        "Stop" | "StopFailure" | "SessionEnd" if agent.is_none() => Some(ActivityEvent::TurnOver),
        _ => None,
    }
}

/// Whether the screen shows the folder-trust dialog a new workspace gets once.
pub fn shows_trust_dialog(screen: &[u8]) -> bool {
    let text = String::from_utf8_lossy(screen).to_lowercase();
    text.contains("trust") && (text.contains("folder") || text.contains("project"))
}

pub const DOWN: &[u8] = b"\x1b[B";
pub const ENTER: &[u8] = b"\r";

/// Which answer of the folder-trust dialog is selected on `screen`, once its options
/// are drawn: `Some(true)` for "Yes". Recent versions select "No, exit" first, so a
/// bare Enter would close Claude.
pub fn trust_dialog_selection(screen: &[u8]) -> Option<bool> {
    if !shows_trust_dialog(screen) {
        return None;
    }
    let text = strip_ansi(&String::from_utf8_lossy(screen)).to_lowercase();
    // The marker as last drawn, and the option right after it.
    let after = &text[text.rfind('\u{276f}')?..];
    match (after.find("yes,"), after.find("no,")) {
        (Some(yes), Some(no)) => Some(yes < no),
        (Some(_), None) => Some(true),
        (None, Some(_)) => Some(false),
        (None, None) => None,
    }
}

/// The text of a terminal stream without its escape sequences (colours, cursor moves).
fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\x1b' {
            out.push(c);
            continue;
        }
        match chars.next() {
            // CSI: parameters, then a final byte in @..~
            Some('[') => {
                for c in chars.by_ref() {
                    if ('@'..='~').contains(&c) {
                        break;
                    }
                }
            }
            // OSC: up to BEL or ESC \\
            Some(']') => {
                while let Some(c) = chars.next() {
                    if c == '\x07' || (c == '\x1b' && chars.next_if_eq(&'\\').is_some()) {
                        break;
                    }
                }
            }
            _ => {}
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_hook_and_the_status_line_post_to_the_daemon() {
        let s = settings(
            "http://127.0.0.1:7420",
            "PROJ-1",
            "sess-1",
            Path::new("/tmp/statusline-sess-1.sh"),
            None,
        );
        for event in HOOK_EVENTS {
            let command = s["hooks"][event][0]["hooks"][0]["command"]
                .as_str()
                .unwrap();
            assert!(
                command.contains("http://127.0.0.1:7420/hooks/claude?task=PROJ-1&session=sess-1"),
                "{command}"
            );
        }
        assert_eq!(s["statusLine"]["command"], "sh '/tmp/statusline-sess-1.sh'");
        let ask = &s["hooks"]["PreToolUse"][0];
        assert_eq!(ask["matcher"], "AskUserQuestion");
        assert!(
            ask["hooks"][0]["command"]
                .as_str()
                .unwrap()
                .contains("/hooks/claude/ask?task=PROJ-1&session=sess-1")
        );
        assert_eq!(ask["hooks"][0]["timeout"], ASK_TIMEOUT_SECS);
    }

    #[test]
    fn the_status_line_forwards_usage_and_then_draws_the_users_own() {
        let script = status_script(
            "http://127.0.0.1:7420",
            "PROJ-1",
            "sess-1",
            Some("bash ~/.claude/statusline.sh"),
        );
        assert!(
            script.contains("/hooks/claude/statusline?task=PROJ-1&session=sess-1"),
            "{script}"
        );
        assert!(script.contains("bash ~/.claude/statusline.sh"), "{script}");
        let plain = status_script("http://127.0.0.1:7420", "PROJ-1", "sess-1", None);
        assert!(plain.contains("Delilah · PROJ-1"), "{plain}");
    }

    #[test]
    fn new_sessions_get_the_id_and_resumed_ones_resume_it() {
        let base = Launch {
            workspace: Path::new("/tmp/ws"),
            session_id: "abc",
            settings: Path::new("/tmp/s.json"),
            prompt: Some("/feature PROJ-1"),
            resume: false,
            extra_dirs: &[],
            read_only: false,
            remote_control: None,
        };
        let args = |l: &Launch| {
            command(l)
                .get_argv()
                .iter()
                .map(|a| a.to_string_lossy().into_owned())
                .collect::<Vec<_>>()
        };

        assert_eq!(
            args(&base),
            [
                "claude",
                "--session-id",
                "abc",
                "--settings",
                "/tmp/s.json",
                "/feature PROJ-1"
            ]
        );
        let resumed = Launch {
            resume: true,
            prompt: Some("continúa"),
            ..base
        };
        assert_eq!(
            args(&resumed),
            [
                "claude",
                "--resume",
                "abc",
                "--settings",
                "/tmp/s.json",
                "continúa"
            ]
        );
        let remote = Launch {
            remote_control: Some("PROJ-1 · Principal"),
            ..base
        };
        assert_eq!(
            args(&remote),
            [
                "claude",
                "--session-id",
                "abc",
                "--remote-control",
                "PROJ-1 · Principal",
                "--settings",
                "/tmp/s.json",
                "/feature PROJ-1"
            ]
        );
        let remote_resumed = Launch {
            resume: true,
            prompt: Some("continúa"),
            ..remote
        };
        assert_eq!(
            args(&remote_resumed),
            [
                "claude",
                "--resume",
                "abc",
                "--remote-control",
                "PROJ-1 · Principal",
                "--settings",
                "/tmp/s.json",
                "continúa"
            ]
        );
        let remote_plan = Launch {
            read_only: true,
            ..remote
        };
        assert_eq!(
            args(&remote_plan),
            [
                "claude",
                "--session-id",
                "abc",
                "--remote-control",
                "PROJ-1 · Principal",
                "--settings",
                "/tmp/s.json",
                "--permission-mode",
                "plan",
                "/feature PROJ-1"
            ]
        );
    }

    #[test]
    fn extra_dirs_go_before_settings_so_the_prompt_stays_a_prompt() {
        let ticket_dir = Path::new("/tmp/tasks/PROJ-1");
        let cmd = command(&Launch {
            workspace: Path::new("/tmp/ws"),
            session_id: "abc",
            settings: Path::new("/tmp/s.json"),
            prompt: Some("/feature PROJ-1"),
            resume: false,
            extra_dirs: &[ticket_dir],
            read_only: false,
            remote_control: None,
        });
        let argv: Vec<String> = cmd
            .get_argv()
            .iter()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            argv,
            [
                "claude",
                "--session-id",
                "abc",
                "--add-dir",
                "/tmp/tasks/PROJ-1",
                "--settings",
                "/tmp/s.json",
                "/feature PROJ-1"
            ]
        );
    }

    #[test]
    fn drops_the_parent_claude_environment() {
        let cmd = command(&Launch {
            workspace: Path::new("/tmp"),
            session_id: "abc",
            settings: Path::new("/tmp/s.json"),
            prompt: None,
            resume: false,
            extra_dirs: &[],
            read_only: false,
            remote_control: None,
        });
        assert!(cmd.get_env("CLAUDE_CODE_CHILD_SESSION").is_none());
        assert!(cmd.get_env("CLAUDECODE").is_none());
        assert_eq!(cmd.get_env("TERM").unwrap(), "xterm-256color");
    }

    #[test]
    fn classifies_hooks() {
        let hook = |name: &str| json!({ "hook_event_name": name, "session_id": "abc" });
        assert_eq!(classify(&hook("Stop")), Signal::TurnEnded);
        assert_eq!(classify(&hook("StopFailure")), Signal::TurnFailed);
        assert_eq!(classify(&hook("Notification")), Signal::NeedsUser);
        assert_eq!(classify(&hook("UserPromptSubmit")), Signal::PromptSubmitted);
        assert_eq!(classify(&hook("PreCompact")), Signal::Other);
    }

    #[test]
    fn spots_the_trust_dialog() {
        assert!(shows_trust_dialog(
            b"Quick safety check: Is this a project you created or one you trust?"
        ));
        assert!(!shows_trust_dialog(b"Welcome to Claude Code"));
    }

    #[test]
    fn sees_which_trust_answer_is_selected() {
        // 2.1.x: "No, exit" comes first and selected; spaces drawn as cursor moves.
        let no_first = "Quick safety check: Is this a project you created or one you trust?\x1b[1C\r\n\
            \x1b[38;5;153m\u{276f}\x1b[39mNo,\x1b[1Cexit\r\n  Yes,\x1b[1CI\x1b[1Ctrust\x1b[1Cthis\x1b[1Cfolder";
        assert_eq!(trust_dialog_selection(no_first.as_bytes()), Some(false));
        // After Down, the last drawing has the marker on Yes.
        let moved = format!("{no_first}\r\n  No, exit\r\n\u{276f} Yes, I trust this folder");
        assert_eq!(trust_dialog_selection(moved.as_bytes()), Some(true));
        let yes_first =
            "Do you trust the files in this folder?\r\n \u{276f} Yes, proceed\r\n   No, exit";
        assert_eq!(trust_dialog_selection(yes_first.as_bytes()), Some(true));
        // The question is up but the options are not drawn yet: wait.
        assert_eq!(
            trust_dialog_selection(b"Do you trust the files in this folder?"),
            None
        );
    }

    #[test]
    fn settings_register_the_activity_hooks() {
        let s = settings("http://d", "T-1", "s1", Path::new("/tmp/s.sh"), None);
        for event in ["PostToolUseFailure", "SubagentStart", "SubagentStop"] {
            let c = s["hooks"][event][0]["hooks"][0]["command"]
                .as_str()
                .unwrap();
            assert!(
                c.contains("http://d/hooks/claude?task=T-1&session=s1"),
                "{c}"
            );
        }
        let pre = s["hooks"]["PreToolUse"].as_array().unwrap();
        assert_eq!(pre.len(), 2);
        let ask = pre
            .iter()
            .find(|e| e["matcher"] == "AskUserQuestion")
            .unwrap();
        assert!(
            ask["hooks"][0]["command"]
                .as_str()
                .unwrap()
                .contains("/hooks/claude/ask?")
        );
        assert_eq!(ask["hooks"][0]["timeout"], ASK_TIMEOUT_SECS);
        let all = pre.iter().find(|e| e["matcher"] == "*").unwrap();
        let c = all["hooks"][0]["command"].as_str().unwrap();
        assert!(
            c.contains("http://d/hooks/claude?task=T-1&session=s1"),
            "{c}"
        );
        assert!(all["hooks"][0].get("timeout").is_none());
        assert!(!c.contains("86400"));
    }

    #[test]
    fn action_of_gives_kind_and_a_short_target() {
        let long = "x".repeat(100);
        let multibyte = "é".repeat(100);
        let cases: Vec<(&str, Value, ActionKind, Option<String>, bool)> = vec![
            (
                "Edit",
                json!({"file_path": "/a/b/c.rs"}),
                ActionKind::Edit,
                Some("c.rs".into()),
                false,
            ),
            (
                "MultiEdit",
                json!({"file_path": "/a/b/d.rs"}),
                ActionKind::Edit,
                Some("d.rs".into()),
                false,
            ),
            (
                "Write",
                json!({"file_path": "/a/e.rs", "content": "SECRET"}),
                ActionKind::Edit,
                Some("e.rs".into()),
                false,
            ),
            (
                "NotebookEdit",
                json!({"notebook_path": "/a/n.ipynb"}),
                ActionKind::Edit,
                Some("n.ipynb".into()),
                false,
            ),
            (
                "Read",
                json!({"file_path": "/a/r.rs"}),
                ActionKind::Read,
                Some("r.rs".into()),
                false,
            ),
            (
                "NotebookRead",
                json!({"notebook_path": "/a/n.ipynb"}),
                ActionKind::Read,
                Some("n.ipynb".into()),
                false,
            ),
            (
                "Grep",
                json!({"pattern": "foo.*"}),
                ActionKind::Read,
                Some("foo.*".into()),
                false,
            ),
            (
                "Glob",
                json!({"pattern": "**/*.rs"}),
                ActionKind::Read,
                Some("**/*.rs".into()),
                false,
            ),
            ("LS", json!({"path": "/a"}), ActionKind::Read, None, false),
            (
                "WebFetch",
                json!({"url": "https://example.com/x?y=1"}),
                ActionKind::Read,
                Some("example.com".into()),
                false,
            ),
            (
                "WebSearch",
                json!({"query": "rust serde"}),
                ActionKind::Read,
                Some("rust serde".into()),
                false,
            ),
            (
                "Bash",
                json!({"command": "  cargo test\nsecond line"}),
                ActionKind::Run,
                Some("cargo test".into()),
                false,
            ),
            (
                "BashOutput",
                json!({"command": "ls"}),
                ActionKind::Run,
                Some("ls".into()),
                false,
            ),
            ("KillShell", json!({}), ActionKind::Run, None, false),
            (
                "Agent",
                json!({"description": "find it", "run_in_background": true}),
                ActionKind::Delegate,
                Some("find it".into()),
                true,
            ),
            (
                "Task",
                json!({"description": "do", "subagent_type": "x"}),
                ActionKind::Delegate,
                Some("do".into()),
                false,
            ),
            (
                "mcp__jira__search",
                json!({"query": "q"}),
                ActionKind::Other,
                None,
                false,
            ),
            (
                "TodoWrite",
                json!({"todos": []}),
                ActionKind::Other,
                None,
                false,
            ),
            (
                "Bash",
                json!({"command": long}),
                ActionKind::Run,
                Some(format!("{}…", "x".repeat(39))),
                false,
            ),
            (
                "Bash",
                json!({"command": multibyte}),
                ActionKind::Run,
                Some(format!("{}…", "é".repeat(39))),
                false,
            ),
        ];
        for (tool, input, kind, target, background) in cases {
            let a = action_of(tool, &input, "tu1");
            assert_eq!(a.kind, kind, "{tool}");
            assert_eq!(a.target, target, "{tool}");
            assert_eq!(a.background, background, "{tool}");
            assert_eq!(a.tool, tool);
            assert_eq!(a.id, "tu1");
            assert!(!serde_json::to_string(&a).unwrap().contains("SECRET"));
        }
        let a = action_of("Bash", &json!({"command": "x".repeat(100)}), "");
        assert_eq!(a.target.unwrap().chars().count(), 40);
    }

    #[test]
    fn ask_user_question_is_not_an_activity() {
        for ev in ["PreToolUse", "PostToolUse", "PostToolUseFailure"] {
            let h = json!({"hook_event_name": ev, "tool_name": "AskUserQuestion",
                "tool_input": {}, "tool_use_id": "t1"});
            assert!(classify_activity(&h).is_none(), "{ev}");
        }
    }

    #[test]
    fn webfetch_target_drops_userinfo_and_keeps_port() {
        let a = action_of("WebFetch", &json!({"url": "https://u:p@example.com:8080/x"}), "t");
        assert_eq!(a.target.as_deref(), Some("example.com:8080"));
    }

    #[test]
    fn classify_activity_maps_hooks_to_events() {
        let pre = json!({"hook_event_name": "PreToolUse", "tool_name": "Bash",
            "tool_input": {"command": "ls"}, "tool_use_id": "t1"});
        match classify_activity(&pre) {
            Some(ActivityEvent::ToolStarted {
                agent: None,
                action,
            }) => {
                assert_eq!(action.id, "t1");
                assert_eq!(action.kind, ActionKind::Run);
            }
            other => panic!("{other:?}"),
        }
        let sub = json!({"hook_event_name": "PreToolUse", "tool_name": "Read",
            "tool_input": {"file_path": "/a/b"}, "tool_use_id": "t2",
            "agent_id": "a1", "agent_type": "Explore"});
        match classify_activity(&sub) {
            Some(ActivityEvent::ToolStarted { agent: Some(r), .. }) => {
                assert_eq!(
                    r,
                    AgentRef {
                        id: "a1".into(),
                        role: "Explore".into()
                    }
                );
            }
            other => panic!("{other:?}"),
        }
        for ev in ["PostToolUse", "PostToolUseFailure"] {
            let h = json!({"hook_event_name": ev, "tool_use_id": "t1"});
            assert_eq!(
                classify_activity(&h),
                Some(ActivityEvent::ToolEnded {
                    agent: None,
                    id: "t1".into()
                })
            );
        }
        let start =
            json!({"hook_event_name": "SubagentStart", "agent_id": "a1", "agent_type": "Explore"});
        assert_eq!(
            classify_activity(&start),
            Some(ActivityEvent::HelperStarted {
                agent: AgentRef {
                    id: "a1".into(),
                    role: "Explore".into()
                }
            })
        );
        let stop = json!({"hook_event_name": "SubagentStop", "agent_id": "a1"});
        assert_eq!(
            classify_activity(&stop),
            Some(ActivityEvent::HelperEnded { id: "a1".into() })
        );
        for ev in ["Stop", "StopFailure", "SessionEnd"] {
            let h = json!({"hook_event_name": ev});
            assert_eq!(classify_activity(&h), Some(ActivityEvent::TurnOver));
            let h = json!({"hook_event_name": ev, "agent_id": "a1"});
            assert_eq!(classify_activity(&h), None);
        }
        assert_eq!(
            classify_activity(&json!({"hook_event_name": "Notification"})),
            None
        );
    }

    #[test]
    fn classify_ignores_the_activity_only_events() {
        for ev in ["PreToolUse", "SubagentStart", "SubagentStop"] {
            assert_eq!(classify(&json!({"hook_event_name": ev})), Signal::Other);
        }
    }
}
