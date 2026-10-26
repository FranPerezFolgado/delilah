//! Claude Code adapter. Findings behind these choices: `docs/archive/spikes/2026-10-19-claude-pty.md`.

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use portable_pty::CommandBuilder;
use serde::Serialize;
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

/// How much the agent may do without asking.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Permission {
    Default,
    /// Auto mode (`--permission-mode auto`): Claude's own classifier decides which
    /// actions go ahead without asking and which still ask. That judgement is Claude's,
    /// and choosing this mode is the user's choice (the `agent-permissions` preference).
    Auto,
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
    /// `--model`: a Claude Code alias (`opus`, `sonnet`…); `None` leaves the account's default.
    pub model: Option<&'a str>,
}

/// Whether Claude saved a conversation for `session_id` started in `workspace`, i.e.
/// whether `--resume` will find it: a session closed before its first message has none.
pub fn has_conversation_in(projects_root: &Path, workspace: &Path, session_id: &str) -> bool {
    crate::transcript::transcript_path_under(projects_root, workspace, session_id)
        .is_some_and(|path| path.is_file())
}

/// The arguments both runtimes share: the session, folders, settings, model, permission
/// mode, plugin and system prompt. `remote_control` is left out in chat mode (it has no terminal).
fn shared_args(launch: &Launch, with_remote_control: bool) -> Vec<OsString> {
    let mut args: Vec<OsString> = Vec::new();
    if launch.resume {
        args.extend(["--resume".into(), launch.session_id.into()]);
    } else {
        args.extend(["--session-id".into(), launch.session_id.into()]);
    }
    // `--add-dir` takes several values: keep another option after it so it never
    // swallows the prompt.
    for dir in launch.extra_dirs {
        args.extend(["--add-dir".into(), dir.into()]);
    }
    // The name is always given: `--remote-control` alone would take the prompt as one.
    if let Some(name) = launch.remote_control.filter(|_| with_remote_control) {
        args.extend(["--remote-control".into(), name.into()]);
    }
    args.extend(["--settings".into(), launch.settings.into()]);
    if let Some(model) = launch.model {
        args.extend(["--model".into(), model.into()]);
    }
    match launch.permission {
        Permission::Default => {}
        Permission::Auto => {
            args.extend(["--permission-mode".into(), "auto".into()]);
        }
        Permission::Plan => {
            args.extend(["--permission-mode".into(), "plan".into()]);
        }
        Permission::DontAsk => {
            args.extend(["--permission-mode".into(), "dontAsk".into()]);
            // A cloned repo's own settings must not widen the allow list.
            args.extend(["--setting-sources".into(), "user".into()]);
            // Nor may the project's or user's MCP servers reach an idea conversation.
            args.push("--strict-mcp-config".into());
        }
    }
    if let Some(dir) = launch.plugin_dir {
        args.extend(["--plugin-dir".into(), dir.into()]);
    }
    if let Some(text) = launch.append_system_prompt {
        args.extend(["--append-system-prompt".into(), text.into()]);
    }
    args
}

fn program<'a>(launch: &Launch<'a>) -> &'a OsStr {
    launch.program.map_or(OsStr::new("claude"), Path::as_os_str)
}

pub fn command(launch: &Launch) -> CommandBuilder {
    let mut cmd = CommandBuilder::new(program(launch));
    cmd.env_clear();
    for (key, value) in crate::env::agent_env() {
        cmd.env(key, value);
    }
    cmd.env("TERM", "xterm-256color");
    cmd.env("COLORTERM", "truecolor");
    cmd.args(shared_args(launch, true));
    if let Some(prompt) = launch.prompt {
        // After `--`, so a prompt starting with `-` is never read as an option.
        cmd.args(["--", prompt]);
    }
    cmd.cwd(launch.workspace);
    cmd
}

/// The command for chat mode: the same environment and arguments, speaking stream-json
/// on stdin and stdout. No positional prompt (the first prompt goes on stdin after the
/// init line, also in `launch.prompt`) and no Remote Control. `--permission-prompt-tool
/// stdio` makes `can_use_tool` requests arrive and adds AskUserQuestion to the tools.
pub fn stream_command(launch: &Launch) -> tokio::process::Command {
    let mut cmd = tokio::process::Command::new(program(launch));
    cmd.env_clear();
    cmd.envs(crate::env::agent_env());
    cmd.args([
        "-p",
        "--input-format",
        "stream-json",
        "--output-format",
        "stream-json",
        "--verbose",
        "--include-partial-messages",
        "--permission-prompt-tool",
        "stdio",
    ]);
    cmd.args(shared_args(launch, false));
    cmd.current_dir(launch.workspace);
    cmd
}

/// Safe, bounded commands a remembered Bash PREFIX rule (`Bash(<prefix>:*)`) may cover.
/// Each entry is a word sequence; a prefix is accepted when it starts with one of them
/// (so `git diff --stat:*` is fine and `git:*` is not). Anything else is remembered only
/// as the exact command. To extend: add an entry of words that, followed by ANY arguments,
/// can neither run arbitrary code (no `-exec`, `--pre`, `-c alias=`, scripts, wrappers)
/// nor write outside the workspace.
pub const SAFE_BASH_PREFIXES: &[&[&str]] = &[
    &["ls"],
    &["cat"],
    &["head"],
    &["tail"],
    &["wc"],
    &["grep"],
    &["pwd"],
    &["echo"],
    &["which"],
    &["tree"],
    &["diff"],
    &["stat"],
    &["file"],
    &["git", "status"],
    &["git", "diff"],
    &["git", "log"],
    &["git", "show"],
    &["git", "add"],
    &["npm", "test"],
    &["pnpm", "test"],
    &["yarn", "test"],
    &["cargo", "test"],
    &["cargo", "check"],
    &["cargo", "build"],
    &["cargo", "clippy"],
    &["cargo", "fmt", "--check"],
    &["go", "test"],
    &["go", "vet"],
    &["go", "build"],
    &["pytest"],
    &["make", "test"],
];

/// First words that hand over arbitrary execution: not even an exact command starting with
/// one is remembered (`sh x.sh` runs whatever the file holds at that moment).
const UNSAFE_BASH_HEADS: &[&str] = &[
    "sh",
    "bash",
    "zsh",
    "dash",
    "fish",
    "env",
    "sudo",
    "doas",
    "xargs",
    "eval",
    "exec",
    "nohup",
    "time",
    "command",
    "builtin",
    "nice",
    "timeout",
    "watch",
    "node",
    "nodejs",
    "python",
    "python2",
    "python3",
    "ruby",
    "perl",
    "php",
    "lua",
    "deno",
    "bun",
    "npx",
    "bunx",
    "pnpx",
    "osascript",
];

/// Flags that make an allowlisted command run a program of the agent's choosing
/// (`go test -exec sh`, `cargo test --config target.x.runner=…`, `npm test --script-shell`).
const EXEC_FLAGS: &[&str] = &[
    "-exec",
    "--exec",
    "-toolexec",
    "--toolexec",
    "-vettool",
    "--vettool",
    "--config",
    "--script-shell",
    "--ext-diff",
    "--textconv",
];

/// Whether the arguments after a remembered Bash prefix stay bounded, checked by the daemon
/// before it answers a request with a `Bash(<prefix>:*)` rule: whole words after the
/// prefix, no chaining, substitution, redirection, quoting or globbing, no flag of
/// [`EXEC_FLAGS`] (nor cargo's `-Z` or `+toolchain`), and every path relative and inside
/// the workspace (no leading `/` or `~`, no `..`), so `cat ~/.ssh/id_rsa` or
/// `git diff --output=../x` ask again.
pub fn is_safe_prefix_tail(tail: &str) -> bool {
    if !(tail.is_empty() || tail.starts_with(char::is_whitespace))
        || tail.contains(SHELL_META)
        || tail.contains(['\'', '"', '*', '?', '[', ']'])
    {
        return false;
    }
    tail.split_whitespace().all(|word| {
        let (name, value) = match word.split_once('=') {
            Some((name, value)) if word.starts_with('-') => (name, Some(value)),
            _ => (word, None),
        };
        let escapes = |s: &str| {
            s.starts_with(['/', '~']) || s.contains(":~") || s.split('/').any(|c| c == "..")
        };
        let flag = name.starts_with('-');
        !(word.starts_with('+')
            || EXEC_FLAGS.contains(&name)
            || name.starts_with("-Z")
            // A short flag with its value glued on (`-o/etc/x`) can't be told from a path.
            || (flag && name.contains('/'))
            || (!flag && escapes(name))
            || value.is_some_and(escapes))
    })
}

/// Characters that chain, substitute or redirect in a shell command (or break a rule).
const SHELL_META: &[char] = &[
    '(', ')', '\n', '\r', ';', '&', '|', '`', '$', '<', '>', '{', '}', '!', '\\',
];

/// Tools whose rule content is a path (inside the workspace only).
pub const PATH_TOOLS: &[&str] = &[
    "Read",
    "Edit",
    "Write",
    "MultiEdit",
    "NotebookEdit",
    "Glob",
    "Grep",
];

/// `NAME=value`, a shell environment assignment.
fn is_env_assignment(word: &str) -> bool {
    word.split_once('=').is_some_and(|(name, _)| {
        !name.is_empty()
            && !name.starts_with(|c: char| c.is_ascii_digit())
            && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
    })
}

/// Whether `command` (a plain command line) is one Delilah may remember exactly: no
/// wildcard, chaining, substitution or redirection, no leading `VAR=value`, and not
/// starting with an interpreter or wrapper.
pub fn is_safe_exact_command(command: &str) -> bool {
    let command = command.trim();
    if command.is_empty() || command.contains('*') || command.contains(SHELL_META) {
        return false;
    }
    let head = command.split_whitespace().next().unwrap_or_default();
    if is_env_assignment(head) {
        return false;
    }
    let head = head.rsplit('/').next().unwrap_or(head);
    !UNSAFE_BASH_HEADS.contains(&head)
}

/// Whether a Bash prefix (without `:*`) is a plain command line at least as specific as an
/// entry of [`SAFE_BASH_PREFIXES`].
pub fn is_safe_bash_prefix(prefix: &str) -> bool {
    let prefix = prefix.trim();
    if !is_safe_exact_command(prefix) {
        return false;
    }
    let words: Vec<&str> = prefix.split_whitespace().collect();
    SAFE_BASH_PREFIXES
        .iter()
        .any(|entry| words.len() >= entry.len() && words[..entry.len()] == **entry)
}

/// Whether a path rule content stays inside the workspace: relative, no `~`, no `//`
/// (absolute), no `..`, and no wildcard but a trailing `/**` after a real directory.
pub fn is_workspace_path(content: &str) -> bool {
    let content = content.trim();
    if content.is_empty() || content.starts_with(['/', '~', '!', '#']) {
        return false;
    }
    let path = content.strip_suffix("/**").unwrap_or(content);
    !path.is_empty()
        && !path.contains(['*', '?', '[', ']', '\\', '{', '}', '\n', '\r', '(', ')'])
        && !path.contains("//")
        && path
            .split('/')
            .all(|c| !c.is_empty() && c != ".." && c != ".")
}

/// Whether a WebFetch content is `domain:<exact host>`.
fn is_exact_domain(content: &str) -> bool {
    content.trim().strip_prefix("domain:").is_some_and(|host| {
        !host.is_empty()
            && host.contains('.')
            && !host.starts_with(['.', '-'])
            && host
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-')
    })
}

/// Structural check of an allow rule before it is written into a session's settings (the
/// daemon applies the same one before remembering): `Tool(content)` only, never a bare
/// tool name, which would allow every use. Bash: `<prefix>:*` only for a prefix on the
/// [`SAFE_BASH_PREFIXES`] allowlist, else one exact plain command. File tools: a path
/// relative to the workspace (so inside it), wildcards only as a trailing `/**`.
/// WebFetch: `domain:<exact host>`. Other tools (MCP): content that is not just wildcards.
pub fn is_safe_allow_rule(rule: &str) -> bool {
    let rule = rule.trim();
    let Some((tool, rest)) = rule.split_once('(') else {
        return false;
    };
    let Some(content) = rest.strip_suffix(')') else {
        return false;
    };
    if tool.is_empty()
        || !tool
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        || content.trim().is_empty()
        || content.contains(['(', ')', '\n', '\r'])
    {
        return false;
    }
    if PATH_TOOLS.contains(&tool) {
        return is_workspace_path(content);
    }
    if tool == "WebFetch" {
        return is_exact_domain(content);
    }
    if tool != "Bash" {
        let rest = content.trim();
        return !rest
            .chars()
            .all(|c| matches!(c, '*' | '/' | '.' | ':') || c.is_whitespace());
    }
    match content.strip_suffix(":*") {
        Some(prefix) => is_safe_bash_prefix(prefix),
        None => is_safe_exact_command(content),
    }
}

/// Settings for one session: every hook posts its JSON to the daemon, and the status
/// line runs `status_script` (see [`status_script`]). Lives outside the repo and is
/// passed with `--settings`. `allowed_tools` are the task's extra allow rules.
#[allow(clippy::too_many_arguments)]
pub fn settings(
    daemon_url: &str,
    task: &str,
    session: &str,
    status_script: &Path,
    padding: Option<&Value>,
    permission: Permission,
    extra_dirs: &[&Path],
    allowed_tools: &[String],
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
    // Every other tool start feeds the activity view; the answer is not awaited.
    pre_tool_use.push(json!({ "matcher": "*", "hooks": [{ "type": "command", "command": post }] }));
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
    } else if permission == Permission::Default {
        // The rules the user chose with "always allow" in this task. Only a session
        // that may edit gets them: a read-only one never gains a write through them.
        // Bash prefix rules stay out: Claude would match any arguments after the prefix
        // (`go test -exec sh`), so only the daemon applies them, with
        // [`is_safe_prefix_tail`], when a chat-mode session asks.
        let allow: Vec<&str> = allowed_tools
            .iter()
            .map(|r| r.trim())
            .filter(|r| is_safe_allow_rule(r) && !is_bash_prefix_rule(r))
            .collect();
        if !allow.is_empty() {
            settings["permissions"] = json!({ "allow": allow });
        }
    }
    settings
}

/// `Bash(<prefix>:*)`.
fn is_bash_prefix_rule(rule: &str) -> bool {
    rule.strip_prefix("Bash(")
        .and_then(|r| r.strip_suffix(')'))
        .is_some_and(|c| c.trim_end().ends_with(":*"))
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

#[allow(clippy::too_many_arguments)]
pub fn write_settings(
    path: &Path,
    daemon_url: &str,
    task: &str,
    session: &str,
    permission: Permission,
    extra_dirs: &[&Path],
    allowed_tools: &[String],
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
        allowed_tools,
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
        /// The tool call failed (`PostToolUseFailure`).
        failed: bool,
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
        ev @ ("PostToolUse" | "PostToolUseFailure") => Some(ActivityEvent::ToolEnded {
            agent,
            id: tool_use_id,
            failed: ev == "PostToolUseFailure",
        }),
        "SubagentStart" => agent.map(|agent| ActivityEvent::HelperStarted { agent }),
        "SubagentStop" => agent.map(|a| ActivityEvent::HelperEnded { id: a.id }),
        "Stop" | "StopFailure" | "SessionEnd" if agent.is_none() => Some(ActivityEvent::TurnOver),
        _ => None,
    }
}

/// Whether Claude Code's `Stop` hook reports a background task still running.
/// The payload carries `background_tasks`, a list of entries with a `status`;
/// only `status == "running"` counts. Any other shape (field missing, not an
/// array, empty, no running entry) means nothing is running. The field is
/// observed in practice, not documented by Claude Code.
pub fn background_running(hook: &Value) -> bool {
    hook["background_tasks"].as_array().is_some_and(|tasks| {
        tasks
            .iter()
            .any(|t| t["status"].as_str() == Some("running"))
    })
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
        // Without the guard, only the activity hook joins it on PreToolUse.
        let pre = s["hooks"]["PreToolUse"].as_array().unwrap();
        assert_eq!(pre.len(), 2);
        assert_eq!(pre[1]["matcher"], "*");
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
            &[],
            true,
        );
        let pre = s["hooks"]["PreToolUse"].as_array().unwrap();
        assert_eq!(pre.len(), 3);
        assert_eq!(pre[0]["matcher"], "AskUserQuestion");
        assert_eq!(pre[1]["matcher"], "Edit|Write|MultiEdit|NotebookEdit|Bash");
        assert_eq!(pre[2]["matcher"], "*");
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
            model: None,
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
            model: None,
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
            model: None,
        }
    }

    fn has_pair(args: &[String], flag: &str, value: &str) -> bool {
        args.windows(2).any(|w| w[0] == flag && w[1] == value)
    }

    #[test]
    fn stream_command_has_the_stream_flags_and_no_prompt_nor_remote_control() {
        let l = Launch {
            prompt: Some("hola"),
            remote_control: Some("PROJ-1"),
            permission: Permission::Plan,
            ..launch()
        };
        let cmd = stream_command(&l);
        let std = cmd.as_std();
        assert_eq!(std.get_program(), "claude");
        let args: Vec<String> = std
            .get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            args,
            [
                "-p",
                "--input-format",
                "stream-json",
                "--output-format",
                "stream-json",
                "--verbose",
                "--include-partial-messages",
                "--permission-prompt-tool",
                "stdio",
                "--session-id",
                "abc",
                "--settings",
                "/tmp/s.json",
                "--permission-mode",
                "plan"
            ]
        );
        assert_eq!(std.get_current_dir(), Some(Path::new("/tmp/ws")));
        let resumed = stream_command(&Launch {
            resume: true,
            program: Some(Path::new("/x/fake")),
            model: None,
            ..launch()
        });
        assert_eq!(resumed.as_std().get_program(), "/x/fake");
        assert!(resumed.as_std().get_args().any(|a| a == "--resume"));
    }

    #[test]
    fn a_model_is_passed_to_both_runtimes_and_none_passes_nothing() {
        let opus = Launch {
            model: Some("opus"),
            prompt: Some("/feature PROJ-1"),
            ..launch()
        };
        let pty = argv(&opus);
        assert!(has_pair(&pty, "--model", "opus"), "{pty:?}");
        // Before the prompt, which comes after `--`.
        let dashes = pty.iter().position(|a| a == "--").unwrap();
        assert!(pty.iter().position(|a| a == "--model").unwrap() < dashes);
        let stream: Vec<String> = stream_command(&opus)
            .as_std()
            .get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        assert!(has_pair(&stream, "--model", "opus"), "{stream:?}");
        // Resuming keeps whatever model it is given (the caller passes the session's own).
        let resumed = argv(&Launch {
            resume: true,
            model: Some("sonnet"),
            ..launch()
        });
        assert!(has_pair(&resumed, "--model", "sonnet"), "{resumed:?}");
        assert!(!argv(&launch()).contains(&"--model".to_string()));
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
    fn auto_mode_reaches_both_runtimes_and_widens_nothing_else() {
        let l = Launch {
            permission: Permission::Auto,
            ..launch()
        };
        let pty = argv(&l);
        assert!(has_pair(&pty, "--permission-mode", "auto"), "{pty:?}");
        let chat: Vec<String> = stream_command(&l)
            .as_std()
            .get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        assert!(has_pair(&chat, "--permission-mode", "auto"), "{chat:?}");
        for args in [&pty, &chat] {
            assert!(!args.contains(&"--setting-sources".to_string()), "{args:?}");
            assert!(
                !args.contains(&"--strict-mcp-config".to_string()),
                "{args:?}"
            );
        }
        assert!(!argv(&launch()).contains(&"--permission-mode".to_string()));
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
                &[],
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
    fn a_prefix_tail_stays_bounded() {
        for ok in [
            "",
            " ./...",
            " -p delilah-daemon --lib",
            " -- --nocapture",
            " --stat HEAD~3",
            " -la src",
            " -rn foo crates/x.rs",
            " --output=out/diff.txt",
            " -v -run TestX ./pkg/...",
        ] {
            assert!(is_safe_prefix_tail(ok), "{ok:?}");
        }
        for bad in [
            "x",
            " -exec /bin/sh",
            " -exec sh",
            " -toolexec=sh",
            " -vettool=x",
            " --config target.x.runner=\"sh\"",
            " --config=build.rustc-wrapper=x",
            " -Zunstable-options",
            " +nightly",
            " --script-shell=python",
            " --ext-diff",
            " -\"exec\" sh",
            " -'exec' sh",
            " ~/.ssh/id_rsa",
            " /etc/passwd",
            " ../../secret",
            " src/../../x",
            " --output=../x",
            " --output=/Users/x/.zshrc",
            " --output=~/x",
            " -o/etc/x",
            " .*",
            " a; sh",
            " $(id)",
            " a > f",
        ] {
            assert!(!is_safe_prefix_tail(bad), "{bad:?}");
        }
    }

    #[test]
    fn a_tasks_allowed_tools_go_into_the_allow_list_of_an_editing_session_only() {
        let rules = vec![
            "Bash(npm test:*)".to_string(),
            " Bash(git status) ".to_string(),
            "Bash(git:*)".to_string(),
            "Edit(~/**)".to_string(),
            "Bash".to_string(),
            "Read".to_string(),
            "Bash(sh:*)".to_string(),
            "Bash(npm test; rm -rf /:*)".to_string(),
            "  ".to_string(),
        ];
        let s = |p| {
            settings(
                "http://x",
                "T-1",
                "s",
                Path::new("/tmp/s.sh"),
                None,
                p,
                &[],
                &rules,
                false,
            )
        };
        assert_eq!(
            s(Permission::Default)["permissions"]["allow"],
            json!(["Bash(git status)"])
        );
        assert!(s(Permission::Default)["permissions"].get("deny").is_none());
        assert!(s(Permission::Plan).get("permissions").is_none());
        let dont_ask = s(Permission::DontAsk);
        assert!(
            !dont_ask["permissions"]["allow"]
                .to_string()
                .contains("npm install")
        );
    }

    #[test]
    fn only_specific_safe_rules_pass_is_safe_allow_rule() {
        for ok in [
            "Bash(npm test:*)",
            "Bash(cargo test:*)",
            "Bash(cargo test -p foo:*)",
            "Bash(git status:*)",
            "Bash(git diff --stat:*)",
            "Bash(ls:*)",
            "Bash(ls -la:*)",
            "Bash(git status)",
            "Bash(npm install lodash)",
            "Bash(rm -rf target)",
            "WebFetch(domain:docs.rs)",
            "Edit(src/a.rs)",
            "Edit(src/**)",
            "Read(README.md)",
            "mcp__srv__tool(read x)",
        ] {
            assert!(is_safe_allow_rule(ok), "{ok}");
        }
        for bad in [
            "",
            "Bash",
            "Edit",
            "WebFetch",
            "Bash()",
            "Bash(:*)",
            "Bash(*)",
            "Bash(npm*:*)",
            "Bash(ls *:*)",
            "Bash(ls;:*)",
            "Bash(a && b:*)",
            "Bash(a | b:*)",
            "Bash(echo `id`:*)",
            "Bash(echo $(id):*)",
            "Bash(a > f:*)",
            "Bash(sh:*)",
            "Bash(bash -c:*)",
            "Bash(env:*)",
            "Bash(sudo ls:*)",
            "Bash(/usr/bin/env FOO=1:*)",
            "Bash(node -e:*)",
            "Bash(python -c:*)",
            "Bash(xargs rm:*)",
            "Bash(ls\nrm:*)",
            // The review's bypasses: not on the allowlist.
            "Bash(git:*)",
            "Bash(npm run:*)",
            "Bash(npm:*)",
            "Bash(find:*)",
            "Bash(git config:*)",
            "Bash(rm:*)",
            "Bash(curl:*)",
            "Bash(git -c core.pager=sh:*)",
            "Bash(cargo:*)",
            "Bash(make:*)",
            // Env assignments and interpreters, also as exact commands.
            "Bash(FOO=1 sh:*)",
            "Bash(FOO=1 ls:*)",
            "Bash(FOO=1 ls)",
            "Bash(sh x.sh)",
            "Bash(node script.js)",
            "WebFetch(*)",
            "WebFetch(domain:*)",
            "WebFetch(domain:*.com)",
            "WebFetch(domain:com)",
            "Edit(~/**)",
            "Write(//**/*.sh)",
            "Read(~/.ssh/**)",
            "Edit(*.rs)",
            "Edit(src/*.rs)",
            "Edit(**/*.rs)",
            "Edit(/etc/passwd)",
            "Edit(//etc/passwd)",
            "Edit(../x)",
            "Edit(src/../../x)",
            "Edit(**)",
            "Read(**)",
            "Read(//**)",
            "Read(  )",
        ] {
            assert!(!is_safe_allow_rule(bad), "{bad:?}");
        }
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
            model: None,
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
            model: None,
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
            model: None,
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
    fn background_running_reads_the_stop_payload() {
        let stop = |tasks: Value| json!({ "hook_event_name": "Stop", "background_tasks": tasks });
        // A running entry, alone or among finished ones.
        assert!(background_running(&stop(json!([{ "status": "running" }]))));
        assert!(background_running(&stop(json!([
            { "status": "completed" },
            { "status": "running" },
            { "status": "failed" }
        ]))));
        // Nothing running.
        assert!(!background_running(&stop(json!([
            { "status": "completed" },
            { "status": "failed" }
        ]))));
        assert!(!background_running(&stop(json!([]))));
        // Missing, not an array, or an entry without a status.
        assert!(!background_running(&json!({ "hook_event_name": "Stop" })));
        assert!(!background_running(&stop(json!("running"))));
        assert!(!background_running(&stop(json!({ "status": "running" }))));
        assert!(!background_running(&stop(json!([{ "id": "x" }]))));
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
        let s = settings(
            "http://d",
            "T-1",
            "s1",
            Path::new("/tmp/s.sh"),
            None,
            Permission::Default,
            &[],
            &[],
            false,
        );
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
        let a = action_of(
            "WebFetch",
            &json!({"url": "https://u:p@example.com:8080/x"}),
            "t",
        );
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
        for (ev, failed) in [("PostToolUse", false), ("PostToolUseFailure", true)] {
            let h = json!({"hook_event_name": ev, "tool_use_id": "t1"});
            assert_eq!(
                classify_activity(&h),
                Some(ActivityEvent::ToolEnded {
                    agent: None,
                    id: "t1".into(),
                    failed
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
