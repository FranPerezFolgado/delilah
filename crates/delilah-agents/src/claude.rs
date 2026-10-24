//! Claude Code adapter. Findings behind these choices: `docs/spikes/2026-10-19-claude-pty.md`.

use std::path::{Path, PathBuf};

use portable_pty::CommandBuilder;
use serde_json::{Value, json};

/// Hook events Delilah listens to.
/// How long AskUserQuestion may wait for an answer from Delilah: a day.
pub const ASK_TIMEOUT_SECS: u64 = 86_400;
/// The tools the branch guard looks at.
pub const GUARD_MATCHER: &str = "Edit|Write|MultiEdit|NotebookEdit|Bash";
/// The guard hook's own timeout: a little over its curl's `-m 15`.
pub const GUARD_TIMEOUT_SECS: u64 = 20;

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

/// How much the agent may do without asking.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Permission {
    Default,
    /// Plan mode: Claude reads and explains, it does not edit.
    Plan,
    /// Anything not allowed is denied instead of asked, and editing tools are denied:
    /// the idea conversation reads code without a terminal prompt in the way.
    DontAsk,
}

/// Tools and commands allowed under [`Permission::DontAsk`]. Each read-only Bash rule
/// also in its RTK-rewritten form: a user-level PreToolUse hook such as RTK rewrites
/// `git status` to `rtk git status`, and allow rules match the rewritten command.
/// No `rg` nor `git log/show/diff`: `rg --pre` runs programs and `--output` writes files.
/// Reading is scoped: the workspace (the cwd) and, per extra dir, that folder; see
/// [`dont_ask_allow`]. Claude applies `Read` rules to Grep and Glob too.
const DONT_ASK_ALLOW: &[&str] = &[
    "AskUserQuestion",
    // Only the plugin's skill, as the call is recorded: `{"skill":"delilah:task-from-idea"}`.
    "Skill(delilah:task-from-idea)",
    "TodoWrite",
    "Bash(git status:*)",
    "Bash(ls:*)",
    "Bash(rtk git status:*)",
    "Bash(rtk ls:*)",
];

/// Editing tools, and the places secrets live (deny wins over the scoped allow).
const DONT_ASK_DENY: &[&str] = &[
    "Edit",
    "MultiEdit",
    "Write",
    "NotebookEdit",
    "Read(~/.ssh/**)",
    "Read(~/.aws/**)",
    "Read(~/.gnupg/**)",
    "Read(~/.config/gh/**)",
    "Read(**/.env)",
    "Read(**/.env.*)",
];

/// The allow list under [`Permission::DontAsk`]: reads limited to the workspace
/// (`./**`, relative to the cwd) and each of `extra_dirs` (`//` marks an absolute path).
fn dont_ask_allow(extra_dirs: &[&Path]) -> Vec<String> {
    let mut allow = vec!["Read(./**)".to_string()];
    for dir in extra_dirs {
        let dir = dir.to_string_lossy();
        allow.push(format!("Read(/{}/**)", dir.trim_end_matches('/')));
    }
    allow.extend(DONT_ASK_ALLOW.iter().map(|rule| rule.to_string()));
    allow
}

pub struct Launch<'a> {
    pub workspace: &'a Path,
    pub session_id: &'a str,
    pub settings: &'a Path,
    /// First prompt, sent as soon as the session starts (also when resuming).
    pub prompt: Option<&'a str>,
    pub resume: bool,
    /// Folders outside the workspace the agent may read, e.g. the task's ticket.
    pub extra_dirs: &'a [&'a Path],
    pub permission: Permission,
    /// A plugin folder to load for this session (`--plugin-dir`).
    pub plugin_dir: Option<&'a Path>,
    /// Extra instructions appended to Claude's system prompt, also when resuming.
    pub append_system_prompt: Option<&'a str>,
    /// Remote Control: the session can also be followed and driven from claude.ai
    /// and the Claude app, under this name.
    pub remote_control: Option<&'a str>,
    /// Program to run instead of `claude` (tests use a stand-in); `None` in production.
    pub program: Option<&'a Path>,
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
    let mut cmd = match launch.program {
        Some(program) => CommandBuilder::new(program),
        None => CommandBuilder::new("claude"),
    };
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
    match launch.permission {
        Permission::Default => {}
        Permission::Plan => {
            cmd.args(["--permission-mode", "plan"]);
        }
        Permission::DontAsk => {
            cmd.args(["--permission-mode", "dontAsk"]);
            // A cloned repo's own settings must not widen the allow list.
            cmd.args(["--setting-sources", "user"]);
            // Nor may the project's or user's MCP servers reach an idea conversation.
            cmd.arg("--strict-mcp-config");
        }
    }
    if let Some(dir) = launch.plugin_dir {
        cmd.arg("--plugin-dir");
        cmd.arg(dir);
    }
    if let Some(text) = launch.append_system_prompt {
        cmd.args(["--append-system-prompt", text]);
    }
    if let Some(prompt) = launch.prompt {
        // After `--`, so a prompt starting with `-` is never read as an option.
        cmd.args(["--", prompt]);
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
    permission: Permission,
    extra_dirs: &[&Path],
    guard: bool,
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
    let mut pre_tool_use = vec![json!({
        "matcher": "AskUserQuestion",
        "hooks": [{ "type": "command", "command": ask, "timeout": ASK_TIMEOUT_SECS }]
    })];
    // A task without a branch gets one before an edit or a commit lands (see the
    // daemon's branching.rs); an empty answer lets the tool through.
    if guard {
        let command = format!(
            "curl -sS -m 15 -X POST -H 'content-type: application/json' --data-binary @- \
             '{daemon_url}/hooks/claude/guard?task={task}&session={session}' 2>/dev/null || true"
        );
        pre_tool_use.push(json!({
            "matcher": GUARD_MATCHER,
            "hooks": [{ "type": "command", "command": command, "timeout": GUARD_TIMEOUT_SECS }]
        }));
    }
    hooks.insert("PreToolUse".into(), Value::Array(pre_tool_use));
    let mut status_line =
        json!({ "type": "command", "command": format!("sh '{}'", status_script.display()) });
    if let Some(padding) = padding {
        status_line["padding"] = padding.clone();
    }
    let mut settings = json!({ "hooks": hooks, "statusLine": status_line });
    if permission == Permission::DontAsk {
        settings["permissions"] =
            json!({ "allow": dont_ask_allow(extra_dirs), "deny": DONT_ASK_DENY });
    }
    settings
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
    permission: Permission,
    extra_dirs: &[&Path],
    guard: bool,
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
        permission,
        extra_dirs,
        guard,
    );
    std::fs::write(path, serde_json::to_vec_pretty(&settings)?)?;
    Ok(path.to_path_buf())
}

/// Writes the Delilah plugin (the `task-from-idea` skill) under `dir`, with `version`
/// in its manifest. Overwrites what is there, so it is safe to call on every start.
pub fn write_plugin(dir: &Path, version: &str) -> std::io::Result<()> {
    let mut manifest: Value =
        serde_json::from_str(include_str!("../plugin/.claude-plugin/plugin.json"))?;
    manifest["version"] = json!(version);
    let manifest_dir = dir.join(".claude-plugin");
    std::fs::create_dir_all(&manifest_dir)?;
    std::fs::write(
        manifest_dir.join("plugin.json"),
        serde_json::to_vec_pretty(&manifest)?,
    )?;
    let skill_dir = dir.join("skills/task-from-idea");
    std::fs::create_dir_all(&skill_dir)?;
    std::fs::write(
        skill_dir.join("SKILL.md"),
        include_str!("../plugin/skills/task-from-idea/SKILL.md"),
    )
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
            Permission::Default,
            &[],
            false,
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
        // Without the guard, nothing else is hooked on PreToolUse.
        assert_eq!(s["hooks"]["PreToolUse"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn the_guard_adds_a_second_pre_tool_use_hook() {
        let s = settings(
            "http://127.0.0.1:7420",
            "LIBRE-1",
            "sess-1",
            Path::new("/tmp/statusline-sess-1.sh"),
            None,
            Permission::Default,
            &[],
            true,
        );
        let pre = s["hooks"]["PreToolUse"].as_array().unwrap();
        assert_eq!(pre.len(), 2);
        assert_eq!(pre[0]["matcher"], "AskUserQuestion");
        assert_eq!(pre[1]["matcher"], "Edit|Write|MultiEdit|NotebookEdit|Bash");
        let command = pre[1]["hooks"][0]["command"].as_str().unwrap();
        assert!(command.contains("curl -sS -m 15"), "{command}");
        assert!(
            command.contains("/hooks/claude/guard?task=LIBRE-1&session=sess-1"),
            "{command}"
        );
        assert!(command.ends_with("|| true"), "{command}");
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
            permission: Permission::Default,
            plugin_dir: None,
            append_system_prompt: None,
            remote_control: None,
            program: None,
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
                "--",
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
                "--",
                "continúa"
            ]
        );
        let remote = Launch {
            remote_control: Some("PROJ-1 · Principal"),
            program: None,
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
                "--",
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
                "--",
                "continúa"
            ]
        );
        let remote_plan = Launch {
            permission: Permission::Plan,
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
                "--",
                "/feature PROJ-1"
            ]
        );
    }

    fn argv(l: &Launch) -> Vec<String> {
        command(l)
            .get_argv()
            .iter()
            .map(|a| a.to_string_lossy().into_owned())
            .collect()
    }

    fn launch() -> Launch<'static> {
        Launch {
            workspace: Path::new("/tmp/ws"),
            session_id: "abc",
            settings: Path::new("/tmp/s.json"),
            prompt: None,
            resume: false,
            extra_dirs: &[],
            permission: Permission::Default,
            plugin_dir: None,
            append_system_prompt: None,
            remote_control: None,
            program: None,
        }
    }

    fn has_pair(args: &[String], flag: &str, value: &str) -> bool {
        args.windows(2).any(|w| w[0] == flag && w[1] == value)
    }

    #[test]
    fn permission_modes_map_to_flags() {
        let plan = argv(&Launch {
            permission: Permission::Plan,
            ..launch()
        });
        assert!(has_pair(&plan, "--permission-mode", "plan"), "{plan:?}");
        let dont_ask = argv(&Launch {
            permission: Permission::DontAsk,
            ..launch()
        });
        assert!(
            has_pair(&dont_ask, "--permission-mode", "dontAsk"),
            "{dont_ask:?}"
        );
        assert!(!argv(&launch()).contains(&"--permission-mode".to_string()));
        // Only the read-only conversation drops the repo's own settings.
        assert!(
            has_pair(&dont_ask, "--setting-sources", "user"),
            "{dont_ask:?}"
        );
        assert!(
            dont_ask.contains(&"--strict-mcp-config".to_string()),
            "{dont_ask:?}"
        );
        assert!(!plan.contains(&"--strict-mcp-config".to_string()));
        assert!(!plan.contains(&"--setting-sources".to_string()));
        assert!(!argv(&launch()).contains(&"--setting-sources".to_string()));
    }

    #[test]
    fn a_prompt_goes_after_double_dash_so_it_is_never_an_option() {
        for prompt in ["--dangerously-skip-permissions", "-p", "idea"] {
            let args = argv(&Launch {
                prompt: Some(prompt),
                plugin_dir: Some(Path::new("/tmp/plugin")),
                ..launch()
            });
            assert_eq!(args[args.len() - 2..], ["--", prompt], "{args:?}");
        }
    }

    #[test]
    fn plugin_dir_and_system_prompt_are_optional_flags() {
        let plain = argv(&launch());
        assert!(!plain.contains(&"--plugin-dir".to_string()));
        assert!(!plain.contains(&"--append-system-prompt".to_string()));
        let dir = Path::new("/tmp/plugin");
        for resume in [false, true] {
            let args = argv(&Launch {
                plugin_dir: Some(dir),
                append_system_prompt: Some("be brief"),
                resume,
                ..launch()
            });
            assert!(has_pair(&args, "--plugin-dir", "/tmp/plugin"), "{args:?}");
            assert!(
                has_pair(&args, "--append-system-prompt", "be brief"),
                "{args:?}"
            );
        }
    }

    #[test]
    fn dont_ask_settings_allow_read_only_tools_and_deny_edits() {
        let extra = Path::new("/home/u/.delilah/tasks/T-1");
        let s = |p| {
            settings(
                "http://x",
                "T-1",
                "s",
                Path::new("/tmp/s.sh"),
                None,
                p,
                &[extra],
                false,
            )
        };
        let dont_ask = s(Permission::DontAsk);
        assert_eq!(
            dont_ask["permissions"]["allow"],
            json!([
                "Read(./**)",
                "Read(//home/u/.delilah/tasks/T-1/**)",
                "AskUserQuestion",
                "Skill(delilah:task-from-idea)",
                "TodoWrite",
                "Bash(git status:*)",
                "Bash(ls:*)",
                "Bash(rtk git status:*)",
                "Bash(rtk ls:*)"
            ])
        );
        assert_eq!(
            dont_ask["permissions"]["deny"],
            json!([
                "Edit",
                "MultiEdit",
                "Write",
                "NotebookEdit",
                "Read(~/.ssh/**)",
                "Read(~/.aws/**)",
                "Read(~/.gnupg/**)",
                "Read(~/.config/gh/**)",
                "Read(**/.env)",
                "Read(**/.env.*)"
            ])
        );
        assert!(s(Permission::Plan).get("permissions").is_none());
        assert!(s(Permission::Default).get("permissions").is_none());
    }

    #[test]
    fn write_plugin_is_idempotent_and_sets_the_version() {
        let dir = tempfile::tempdir().unwrap();
        write_plugin(dir.path(), "1.2.3").unwrap();
        write_plugin(dir.path(), "1.2.4").unwrap();
        let manifest: Value = serde_json::from_str(
            &std::fs::read_to_string(dir.path().join(".claude-plugin/plugin.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(manifest["name"], "delilah");
        assert_eq!(manifest["version"], "1.2.4");
        let skill =
            std::fs::read_to_string(dir.path().join("skills/task-from-idea/SKILL.md")).unwrap();
        assert!(skill.contains("name: task-from-idea"), "{skill}");
    }

    #[test]
    fn the_program_override_replaces_claude_as_argv0() {
        let fake = Path::new("/tmp/fake-claude");
        let launch = |program| Launch {
            workspace: Path::new("/tmp/ws"),
            session_id: "abc",
            settings: Path::new("/tmp/s.json"),
            prompt: None,
            resume: false,
            extra_dirs: &[],
            permission: Permission::Default,
            plugin_dir: None,
            append_system_prompt: None,
            remote_control: None,
            program,
        };
        let first = |l: &Launch| command(l).get_argv()[0].to_string_lossy().into_owned();
        assert_eq!(first(&launch(None)), "claude");
        assert_eq!(first(&launch(Some(fake))), "/tmp/fake-claude");
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
            permission: Permission::Default,
            plugin_dir: None,
            append_system_prompt: None,
            remote_control: None,
            program: None,
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
                "--",
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
            permission: Permission::Default,
            plugin_dir: None,
            append_system_prompt: None,
            remote_control: None,
            program: None,
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
