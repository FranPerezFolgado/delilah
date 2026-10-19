//! Claude Code adapter. Findings behind these choices: `docs/spikes/2026-10-19-claude-pty.md`.

use std::path::{Path, PathBuf};

use portable_pty::CommandBuilder;
use serde_json::{Value, json};

/// Hook events Delilah listens to.
/// How long AskUserQuestion may wait for an answer from Delilah: a day.
pub const ASK_TIMEOUT_SECS: u64 = 86_400;

const HOOK_EVENTS: &[&str] = &[
    "SessionStart",
    "UserPromptSubmit",
    "PostToolUse",
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
    cmd.arg("--settings");
    cmd.arg(launch.settings);
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
        json!([{
            "matcher": "AskUserQuestion",
            "hooks": [{ "type": "command", "command": ask, "timeout": ASK_TIMEOUT_SECS }]
        }]),
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

/// The status line the user configured for Claude, looked up the way Claude does:
/// the project's local settings, the project's, then the user's. `--settings` would
/// otherwise replace it, so Delilah runs it itself.
pub fn user_status_line(workspace: &Path) -> Option<Value> {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let candidates = [
        Some(workspace.join(".claude/settings.local.json")),
        Some(workspace.join(".claude/settings.json")),
        home.map(|h| h.join(".claude/settings.json")),
    ];
    candidates.into_iter().flatten().find_map(|path| {
        let settings: Value = serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()?;
        let line = settings.get("statusLine")?;
        (line["type"] == "command" && line["command"].is_string()).then(|| line.clone())
    })
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
    workspace: &Path,
) -> std::io::Result<PathBuf> {
    let dir = path.parent().unwrap_or(Path::new("."));
    std::fs::create_dir_all(dir)?;
    let user = user_status_line(workspace);
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
}
