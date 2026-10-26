//! `delilah`: a client of the daemon's HTTP API. It never touches the database.

mod attach;

use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;

use chrono::{DateTime, Local, Utc};
use clap::{Args, Parser, Subcommand, ValueEnum};
use delilah_core::{
    AutofixDefault, AutopilotState, Event, Project, ProjectSettings, ShareMode, SharedEntry, Task,
    TaskStatus, WorkspaceMode,
};
use reqwest::blocking::{Client, Response};
use serde::Deserialize;
use serde_json::json;

/// Local orchestrator for coding agents.
#[derive(Parser)]
#[command(name = "delilah", version, about)]
struct Cli {
    /// Address of the daemon.
    #[arg(
        long,
        env = "DELILAH_URL",
        default_value = "http://127.0.0.1:7420",
        global = true
    )]
    url: String,
    #[command(subcommand)]
    command: Command,
}

/// The saving-mode choice for a task; neither flag leaves it to the user preference.
#[derive(Args)]
struct SavingModeFlags {
    /// Run the task in saving mode (each phase -- spec and plan, implementation, review -- runs in a fresh Claude session).
    #[arg(long, conflicts_with = "no_saving_mode")]
    saving_mode: bool,
    /// Run the task without saving mode, whatever the preference says.
    #[arg(long)]
    no_saving_mode: bool,
}

impl SavingModeFlags {
    /// `None` when no flag was given: the daemon then keeps what the task has.
    fn choice(&self) -> Option<bool> {
        match (self.saving_mode, self.no_saving_mode) {
            (true, _) => Some(true),
            (_, true) => Some(false),
            _ => None,
        }
    }
}

/// Adds `chat_mode: true` only with `--chat`, so old daemons get the body they know.
fn with_chat(mut body: serde_json::Value, chat: bool) -> serde_json::Value {
    if chat {
        body["chat_mode"] = json!(true);
    }
    body
}

/// Adds `saving_mode` to a request body only when the user chose it.
fn with_saving_mode(mut body: serde_json::Value, choice: Option<bool>) -> serde_json::Value {
    if let Some(on) = choice {
        body["saving_mode"] = json!(on);
    }
    body
}

/// The autopilot choice for a task; neither flag means off for a new task and kept
/// for an existing one (a backlog task started with `run`).
/// Autopilot implies saving mode, so `--autopilot` rules out `--no-saving-mode`.
#[derive(Args)]
struct AutopilotFlags {
    /// Let the agent answer its own questions with the recommended option and carry on to the PR (implies saving mode).
    #[arg(long, conflicts_with_all = ["no_autopilot", "no_saving_mode"])]
    autopilot: bool,
    /// Run the task without autopilot, also clearing one stored on a backlog task.
    #[arg(long)]
    no_autopilot: bool,
}

impl AutopilotFlags {
    /// `None` when no flag was given: the daemon then keeps what the task has.
    fn choice(&self) -> Option<bool> {
        match (self.autopilot, self.no_autopilot) {
            (true, _) => Some(true),
            (_, true) => Some(false),
            _ => None,
        }
    }
}

/// Adds `autopilot` to a request body only when the user chose it.
fn with_autopilot(mut body: serde_json::Value, choice: Option<bool>) -> serde_json::Value {
    if let Some(on) = choice {
        body["autopilot"] = json!(on);
    }
    body
}

/// The CI autofix choice for a task; neither flag means the project's default applies.
#[derive(Args)]
struct AutofixFlags {
    /// Let the agent fix the PR's failing CI checks on its own.
    #[arg(long, conflicts_with = "no_autofix")]
    autofix: bool,
    /// Don't fix the PR's failing CI checks, whatever the project's default says.
    #[arg(long)]
    no_autofix: bool,
}

impl AutofixFlags {
    /// `None` when no flag was given: the daemon applies the project's default.
    fn choice(&self) -> Option<bool> {
        match (self.autofix, self.no_autofix) {
            (true, _) => Some(true),
            (_, true) => Some(false),
            _ => None,
        }
    }
}

/// Adds `ci_autofix` to a request body only when the user chose it.
fn with_ci_autofix(mut body: serde_json::Value, choice: Option<bool>) -> serde_json::Value {
    if let Some(on) = choice {
        body["ci_autofix"] = json!(on);
    }
    body
}

/// What `delilah autofix <KEY>` does.
#[derive(Clone, Copy, ValueEnum)]
enum AutofixAction {
    /// Turns CI autofix on for the task.
    On,
    /// Turns CI autofix off for the task.
    Off,
    /// Starts a new fix attempt for a PR whose checks still fail (needs --repo).
    Retry,
}

impl AutofixAction {
    fn name(self) -> &'static str {
        match self {
            Self::On => "on",
            Self::Off => "off",
            Self::Retry => "retry",
        }
    }
}

/// The body of `POST /tasks/{key}/autofix`; `retry` names the repo of the PR.
fn autofix_body(action: AutofixAction, repo: Option<&str>) -> Result<serde_json::Value, String> {
    match (action, repo) {
        (AutofixAction::Retry, None) => Err(
            "autofix retry needs --repo <REPO> (the repo of the PR, `.` for the task's own)".into(),
        ),
        (AutofixAction::Retry, Some(repo)) => Ok(json!({ "action": "retry", "repo": repo })),
        (action, _) => Ok(json!({ "action": action.name() })),
    }
}

/// What `delilah autopilot <KEY>` does.
#[derive(Clone, Copy, ValueEnum)]
enum AutopilotAction {
    /// Turns autopilot on for the task.
    On,
    /// Turns autopilot off for the task.
    Off,
    /// Resumes an autopilot paused for a permission or a question it could not answer.
    Resume,
}

impl AutopilotAction {
    fn name(self) -> &'static str {
        match self {
            Self::On => "on",
            Self::Off => "off",
            Self::Resume => "resume",
        }
    }
}

/// Where `delilah runtime <KEY>` moves a task's agent.
#[derive(Clone, Copy, ValueEnum)]
enum RuntimeMode {
    /// Chat mode (experimental): no terminal, the conversation is the only view.
    Chat,
    /// The live terminal.
    Terminal,
}

impl RuntimeMode {
    fn name(self) -> &'static str {
        match self {
            Self::Chat => "chat",
            Self::Terminal => "terminal",
        }
    }

    fn session_done_line(self, id: &str) -> String {
        match self {
            Self::Chat => format!("Session {id} now runs in chat mode."),
            Self::Terminal => format!("Session {id} now runs in the terminal."),
        }
    }

    fn done_line(self, key: &str) -> String {
        match self {
            Self::Chat => format!("{key} now runs in chat mode."),
            Self::Terminal => format!("{key} now runs in the terminal."),
        }
    }
}

#[derive(Subcommand)]
enum Command {
    /// Checks that the daemon is running.
    Status,
    /// Lists the tasks.
    Ls,
    /// Saves a task to a project's backlog, without starting it (see `delilah run`).
    New {
        /// The Jira ticket; leave it out with --free.
        key: Option<String>,
        #[arg(long)]
        title: String,
        /// Registered project (see `delilah project ls`).
        #[arg(long, env = "DELILAH_PROJECT")]
        project: String,
        /// What to keep with the task; the agent gets it when the task starts.
        #[arg(long)]
        notes: Option<String>,
        /// A task without a Jira ticket: Delilah names it LIBRE-<n>.
        #[arg(long)]
        free: bool,
        /// Work in the project's checkout once started (default: the project's setting, else the checkout).
        #[arg(long, conflicts_with = "worktree")]
        checkout: bool,
        /// Work in an isolated worktree once started (default: the project's setting, else the checkout).
        #[arg(long)]
        worktree: bool,
        /// Defaults to the project's workflow.
        #[arg(long)]
        workflow: Option<String>,
        /// Run the agent in chat mode (experimental): no terminal, the conversation is the only view.
        #[arg(long)]
        chat: bool,
        #[command(flatten)]
        saving: SavingModeFlags,
        #[command(flatten)]
        autopilot: AutopilotFlags,
        #[command(flatten)]
        autofix: AutofixFlags,
    },
    /// Shows a task and its history.
    Show { key: String },
    /// Sends a raw event to a task, e.g. '{"type":"start"}'. For development.
    Event { key: String, json: String },
    /// Builds a task's workspace and starts Claude on it: a backlog task, or a new
    /// one (then --title is required).
    Run {
        key: String,
        #[arg(long)]
        title: Option<String>,
        /// Registered project (see `delilah project ls`); a backlog task's own if
        /// left out.
        #[arg(long, env = "DELILAH_PROJECT")]
        project: Option<String>,
        /// Parent projects: the submodules the task touches, comma separated.
        #[arg(long, value_delimiter = ',')]
        submodules: Vec<String>,
        /// Defaults to the project's workflow.
        #[arg(long)]
        workflow: Option<String>,
        /// Defaults to the project's branch template if it has one; else a free task starts
        /// without a branch (the first analysis names it), and a Jira task gets a name
        /// Claude proposes from the title (feature/KEY-…). A typed one is normalised and
        /// made unique.
        #[arg(long)]
        branch: Option<String>,
        /// First prompt; defaults to `/feature <KEY>`.
        #[arg(long)]
        prompt: Option<String>,
        /// Work in the project's checkout (default: the backlog task's choice, else the
        /// project's setting, else the checkout).
        #[arg(long, conflicts_with = "worktree")]
        checkout: bool,
        /// Work in an isolated worktree (default: the checkout, unless the project or backlog task says otherwise).
        #[arg(long)]
        worktree: bool,
        /// Attach to the agent's terminal right away.
        #[arg(long)]
        attach: bool,
        /// Run the agent in chat mode (experimental): no terminal, the conversation is the only view.
        #[arg(long)]
        chat: bool,
        #[command(flatten)]
        saving: SavingModeFlags,
        #[command(flatten)]
        autopilot: AutopilotFlags,
        #[command(flatten)]
        autofix: AutofixFlags,
    },
    /// Opens the live terminal of a task's agent (Ctrl-] detaches).
    Attach { key: String },
    /// Moves a task's live agent between chat (experimental) and the terminal, keeping
    /// the conversation. Refused while the agent works or waits for an answer.
    /// With `--session <ID>` it moves only that session (no key needed).
    Runtime {
        /// `<KEY> <chat|terminal>`, or just `<chat|terminal>` with `--session`.
        #[arg(required = true, num_args = 1..=2)]
        words: Vec<String>,
        /// Move only this session instead of the whole task.
        #[arg(long)]
        session: Option<String>,
    },
    /// Reopens a finished task (done) so it can be worked on again.
    Reopen { key: String },
    /// Resumes a task paused by the usage limit now, without waiting for the reset.
    Resume { key: String },
    /// Turns a task's autopilot on or off, or resumes it when paused.
    Autopilot {
        key: String,
        action: AutopilotAction,
    },
    /// Turns a task's CI autofix on or off, or retries it for a PR.
    Autofix {
        key: String,
        action: AutofixAction,
        /// The repo of the PR to retry (`.` for the task's own); required with `retry`.
        #[arg(long)]
        repo: Option<String>,
    },
    /// Marks a task as done and stops its agent.
    Done { key: String },
    /// Removes the workspace of a finished task (a worktree is kept while work is
    /// unpushed, unless --force).
    Archive {
        key: String,
        /// Remove the worktree even if uncommitted or unpushed work is lost.
        #[arg(long)]
        force: bool,
    },
    /// Deletes a task for good: stops its agent, releases its workspace (the branch
    /// stays) and forgets it. Refuses while a worktree has unpushed work, unless --force.
    #[command(alias = "delete")]
    Rm {
        key: String,
        /// Delete it even if uncommitted or unpushed work in its worktree is lost.
        #[arg(long)]
        force: bool,
    },
    /// Shows the subscription's usage windows, as Claude last reported them.
    Usage,
    /// Registers and lists projects.
    #[command(subcommand)]
    Project(ProjectCommand),
    /// Acts on a task whose project setup failed or is running.
    #[command(subcommand)]
    Setup(SetupCommand),
}

#[derive(Subcommand)]
enum SetupCommand {
    /// Runs the setup again after it failed.
    Retry { key: String },
    /// Launches the agent without the setup, after it failed.
    Skip { key: String },
    /// Cancels a running setup (handled like a failure).
    Cancel { key: String },
}

#[derive(Subcommand)]
enum ProjectCommand {
    /// Registers a repo: `parent` (submodules, as at work) or `repo` (one repo).
    Add {
        path: PathBuf,
        #[arg(long, value_parser = ["parent", "repo"])]
        kind: String,
        /// Defaults to the folder name.
        #[arg(long)]
        name: Option<String>,
        #[arg(long, default_value = "feature")]
        workflow: String,
    },
    /// Lists the registered projects.
    Ls,
    /// Shows a project's effective settings; defaults are marked.
    Show { name: String },
    /// Changes one setting of a project; the daemon validates it.
    Set {
        name: String,
        #[command(subcommand)]
        setting: SetCommand,
    },
    /// Takes a setting back to its default.
    Unset { name: String, setting: SettingName },
}

#[derive(Subcommand)]
enum SetCommand {
    /// Where tasks work: `checkout` or `worktree` (unset: checkout).
    Workspace { mode: WorkspaceArg },
    /// Branch template, e.g. 'feature/{ticket}-{slug}' (unset it to let Delilah propose a name).
    Branch { template: String },
    /// Command run in each new workspace.
    Setup { command: String },
    /// Paths of the checkout made available in each workspace.
    #[command(subcommand)]
    Shared(SharedCommand),
    /// Submodules a parent project's tasks always or never include.
    Submodules {
        rule: SubmoduleRule,
        #[command(subcommand)]
        action: SubmoduleAction,
    },
    /// Which tasks get CI autofix: `off`, `autopilot` (only autopilot tasks) or `always` (unset: autopilot).
    CiAutofix { mode: AutofixArg },
}

#[derive(Subcommand)]
enum SharedCommand {
    /// Shares a path (an existing one takes the new mode).
    Add {
        path: String,
        #[command(flatten)]
        mode: ShareFlags,
    },
    /// Stops sharing a path.
    Rm { path: String },
}

#[derive(Args)]
struct ShareFlags {
    /// Copy the path into each workspace.
    #[arg(long, conflicts_with = "link")]
    copy: bool,
    /// Symlink the path into each workspace.
    #[arg(long)]
    link: bool,
}

#[derive(Subcommand)]
enum SubmoduleAction {
    Add { path: String },
    Rm { path: String },
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, ValueEnum)]
enum WorkspaceArg {
    Checkout,
    Worktree,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, ValueEnum)]
enum AutofixArg {
    Off,
    Autopilot,
    Always,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, ValueEnum)]
enum SubmoduleRule {
    Always,
    Never,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, ValueEnum)]
enum SettingName {
    Workspace,
    Branch,
    Setup,
    Shared,
    Submodules,
    CiAutofix,
}

/// One change to a project's settings.
#[derive(Debug, PartialEq)]
enum Change {
    Workspace(WorkspaceArg),
    Branch(String),
    Setup(String),
    SharedAdd(String, Option<ShareMode>),
    SharedRm(String),
    SubmodulesAdd(SubmoduleRule, String),
    SubmodulesRm(SubmoduleRule, String),
    CiAutofix(AutofixArg),
    Unset(SettingName),
}

impl From<SetCommand> for Change {
    fn from(c: SetCommand) -> Self {
        match c {
            SetCommand::Workspace { mode } => Change::Workspace(mode),
            SetCommand::Branch { template } => Change::Branch(template),
            SetCommand::Setup { command } => Change::Setup(command),
            SetCommand::Shared(SharedCommand::Add { path, mode }) => {
                let mode = match (mode.copy, mode.link) {
                    (true, _) => Some(ShareMode::Copy),
                    (_, true) => Some(ShareMode::Link),
                    _ => None,
                };
                Change::SharedAdd(path, mode)
            }
            SetCommand::Shared(SharedCommand::Rm { path }) => Change::SharedRm(path),
            SetCommand::Submodules { rule, action } => match action {
                SubmoduleAction::Add { path } => Change::SubmodulesAdd(rule, path),
                SubmoduleAction::Rm { path } => Change::SubmodulesRm(rule, path),
            },
            SetCommand::CiAutofix { mode } => Change::CiAutofix(mode),
        }
    }
}

/// Applies one change to the current settings. No validation here: the daemon
/// does that. Removing something that is not there is an error.
fn apply_change(mut s: ProjectSettings, change: Change) -> Result<ProjectSettings, String> {
    let defaults = ProjectSettings::default();
    match change {
        Change::Workspace(m) => {
            s.workspace = Some(match m {
                WorkspaceArg::Checkout => WorkspaceMode::Checkout,
                WorkspaceArg::Worktree => WorkspaceMode::Worktree,
            })
        }
        Change::Branch(t) => s.branch = Some(t),
        Change::Setup(c) => s.setup = Some(c),
        Change::SharedAdd(path, mode) => match s.shared.iter_mut().find(|e| e.path == path) {
            Some(entry) => entry.mode = mode,
            None => s.shared.push(SharedEntry { path, mode }),
        },
        Change::SharedRm(path) => {
            let before = s.shared.len();
            s.shared.retain(|e| e.path != path);
            if s.shared.len() == before {
                return Err(format!("shared: {path} is not shared"));
            }
        }
        Change::SubmodulesAdd(rule, path) => {
            let list = rule_list(&mut s, rule);
            if !list.contains(&path) {
                list.push(path);
            }
        }
        Change::SubmodulesRm(rule, path) => {
            let list = rule_list(&mut s, rule);
            let before = list.len();
            list.retain(|p| *p != path);
            if list.len() == before {
                return Err(format!("submodules: {path} is not in the {rule:?} list"));
            }
        }
        Change::CiAutofix(mode) => {
            s.ci_autofix = match mode {
                AutofixArg::Off => AutofixDefault::Off,
                AutofixArg::Autopilot => AutofixDefault::Autopilot,
                AutofixArg::Always => AutofixDefault::Always,
            }
        }
        Change::Unset(name) => match name {
            SettingName::Workspace => s.workspace = defaults.workspace,
            SettingName::Branch => s.branch = defaults.branch,
            SettingName::Setup => s.setup = defaults.setup,
            SettingName::Shared => s.shared = defaults.shared,
            SettingName::Submodules => s.submodules = defaults.submodules,
            SettingName::CiAutofix => s.ci_autofix = defaults.ci_autofix,
        },
    }
    Ok(s)
}

fn rule_list(s: &mut ProjectSettings, rule: SubmoduleRule) -> &mut Vec<String> {
    match rule {
        SubmoduleRule::Always => &mut s.submodules.always,
        SubmoduleRule::Never => &mut s.submodules.never,
    }
}

/// The effective settings, with `(default)` on what equals the default.
fn settings_lines(s: &ProjectSettings) -> Vec<String> {
    let d = ProjectSettings::default();
    let mark = |is_default: bool| if is_default { " (default)" } else { "" };
    let list = |v: &[String]| {
        if v.is_empty() {
            "-".to_string()
        } else {
            v.join(", ")
        }
    };
    let mut lines = vec![
        format!(
            "workspace:  {}{}",
            match s.workspace {
                Some(WorkspaceMode::Checkout) => "checkout",
                Some(WorkspaceMode::Worktree) => "worktree",
                None => "checkout",
            },
            mark(s.workspace == d.workspace)
        ),
        format!(
            "branch:     {}{}",
            s.branch
                .as_deref()
                .unwrap_or("(default: proposed by Delilah)"),
            mark(s.branch == d.branch)
        ),
        format!(
            "setup:      {}{}",
            s.setup.as_deref().unwrap_or("-"),
            mark(s.setup == d.setup)
        ),
        format!(
            "shared:     {}",
            if s.shared.is_empty() {
                "- (default)"
            } else {
                ""
            }
        ),
    ];
    for e in &s.shared {
        let mode = match e.mode {
            Some(ShareMode::Copy) => "copy",
            Some(ShareMode::Link) => "link",
            None => "auto",
        };
        lines.push(format!("  {} ({mode})", e.path));
    }
    let default_subs = s.submodules == d.submodules;
    lines.push(format!("submodules:{}", mark(default_subs)));
    lines.push(format!("  always: {}", list(&s.submodules.always)));
    lines.push(format!("  never:  {}", list(&s.submodules.never)));
    lines.push(format!(
        "ci-autofix: {}{}",
        match s.ci_autofix {
            AutofixDefault::Off => "off",
            AutofixDefault::Autopilot => "autopilot",
            AutofixDefault::Always => "always",
        },
        mark(s.ci_autofix == d.ci_autofix)
    ));
    lines
}

#[derive(Deserialize)]
struct Health {
    version: String,
    /// The daemon's home directory (newer daemons).
    #[serde(default)]
    #[allow(dead_code)]
    home: Option<String>,
}

#[derive(Deserialize)]
struct EventRecord {
    at: DateTime<Utc>,
    event: Event,
}

#[derive(Deserialize)]
struct UsageReport {
    at: DateTime<Utc>,
    task: String,
    rate_limits: serde_json::Value,
}

#[derive(Deserialize)]
struct ErrorBody {
    error: String,
    /// Refused because work in the task's worktree would be lost.
    #[serde(default)]
    unpushed: bool,
    /// A 422 on project settings: one entry per problem.
    #[serde(default)]
    fields: Vec<FieldMessage>,
}

#[derive(Deserialize)]
struct FieldMessage {
    field: String,
    message: String,
}

impl From<String> for ErrorBody {
    fn from(error: String) -> Self {
        Self {
            error,
            unpushed: false,
            fields: Vec::new(),
        }
    }
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    // Creating a workspace clones submodules: allow for slow ones.
    let client = Client::builder()
        .timeout(Duration::from_secs(600))
        .build()
        .expect("HTTP client");
    let api = Api {
        client,
        base: cli.url.trim_end_matches('/').to_string(),
    };
    match run(&api, cli.command) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

/// Reads `runtime`'s words: `<KEY> <mode>`, or `<mode>` alone with `--session`.
/// Returns the task key (None for a session), the mode and the session id.
fn runtime_target(
    words: &[String],
    session: Option<&str>,
) -> Result<(Option<String>, RuntimeMode, Option<String>), String> {
    let parse = |w: &str| {
        RuntimeMode::from_str(w, true)
            .map_err(|_| format!("mode must be chat or terminal, not {w}"))
    };
    match (words, session) {
        ([mode], Some(id)) => Ok((None, parse(mode)?, Some(id.to_string()))),
        ([_, mode], Some(id)) => Ok((None, parse(mode)?, Some(id.to_string()))),
        ([key, mode], None) => Ok((Some(key.clone()), parse(mode)?, None)),
        _ => Err(
            "usage: delilah runtime <KEY> <chat|terminal>, or --session <ID> <chat|terminal>"
                .into(),
        ),
    }
}

fn run(api: &Api, command: Command) -> Result<(), String> {
    match command {
        Command::Status => {
            let health: Health = api.get("/api/v1/health")?;
            println!("delilahd {} at {}", health.version, api.base);
        }
        Command::Ls => {
            let tasks: Vec<Task> = api.get("/api/v1/tasks")?;
            if tasks.is_empty() {
                println!(
                    "No tasks yet. Save one with `delilah new <KEY> --title ... --project ...`"
                );
            }
            for task in tasks {
                println!(
                    "{:<12} {:<14} {:<20} {:<10} {}",
                    task.key,
                    task.project.as_deref().unwrap_or("-"),
                    status_label(&task.status),
                    task.phase.as_deref().unwrap_or("-"),
                    task.title
                );
            }
        }
        Command::New {
            key,
            title,
            project,
            notes,
            free,
            checkout,
            worktree,
            workflow,
            chat,
            saving,
            autopilot,
            autofix,
        } => {
            if key.is_none() && !free {
                return Err("say the ticket's KEY, or --free for a task without one".into());
            }
            let task: Task = api.post(
                "/api/v1/tasks",
                &with_ci_autofix(
                    with_autopilot(
                        with_saving_mode(
                            with_chat(
                                new_body(
                                    key.unwrap_or_default(),
                                    title,
                                    project,
                                    notes,
                                    free,
                                    choice(checkout, worktree),
                                    workflow,
                                ),
                                chat,
                            ),
                            saving.choice(),
                        ),
                        autopilot.choice(),
                    ),
                    autofix.choice(),
                ),
            )?;
            println!(
                "Saved {} to {}'s backlog. Start it with `delilah run {}`.",
                task.key,
                task.project.as_deref().unwrap_or("-"),
                task.key
            );
        }
        Command::Show { key } => {
            let task: Task = api.get(&format!("/api/v1/tasks/{key}"))?;
            let events: Vec<EventRecord> = api.get(&format!("/api/v1/tasks/{key}/events"))?;
            println!("{}  {}", task.key, task.title);
            println!("  workflow: {}", task.workflow);
            println!("  phase:    {}", task.phase.as_deref().unwrap_or("-"));
            println!("  status:   {}", status_label(&task.status));
            println!(
                "  created:  {}",
                task.created_at
                    .with_timezone(&Local)
                    .format("%Y-%m-%d %H:%M:%S")
            );
            println!("  history:");
            for record in events {
                let at = record.at.with_timezone(&Local).format("%Y-%m-%d %H:%M:%S");
                println!("    {at}  {}", record.event.name());
            }
        }
        Command::Event { key, json } => {
            let event: Event =
                serde_json::from_str(&json).map_err(|e| format!("invalid event: {e}"))?;
            let task: Task = api.post(&format!("/api/v1/tasks/{key}/events"), &event)?;
            println!("{} is now {}", task.key, status_label(&task.status));
        }
        Command::Run {
            key,
            title,
            project,
            submodules,
            workflow,
            branch,
            prompt,
            checkout,
            worktree,
            attach,
            chat,
            saving,
            autopilot,
            autofix,
        } => {
            let mut body = json!({
                "key": key, "title": title.unwrap_or_default(), "project": project.unwrap_or_default(),
                "workflow": workflow,
                "submodules": submodules, "branch": branch, "prompt": prompt,
            });
            with_choice(&mut body, choice(checkout, worktree));
            let body = with_ci_autofix(
                with_autopilot(
                    with_saving_mode(with_chat(body, chat), saving.choice()),
                    autopilot.choice(),
                ),
                autofix.choice(),
            );
            let task: Task = api.post("/api/v1/runs", &body)?;
            println!(
                "{} is {}: Claude is working on it.",
                task.key,
                status_label(&task.status)
            );
            if attach {
                attach::attach(&api.base, &key)?;
            } else {
                println!("Watch it with `delilah attach {key}`.");
            }
        }
        Command::Attach { key } => attach::attach(&api.base, &key)?,
        Command::Resume { key } => {
            let task: Task = api.post(&format!("/api/v1/tasks/{key}/resume"), &json!({}))?;
            println!("{} is {} again.", task.key, status_label(&task.status));
        }
        Command::Runtime { words, session } => match runtime_target(&words, session.as_deref())? {
            (None, mode, Some(id)) => {
                api.post_empty(
                    &format!("/api/v1/sessions/{id}/runtime"),
                    &json!({ "mode": mode.name() }),
                )?;
                println!("{}", mode.session_done_line(&id));
            }
            (Some(key), mode, _) => {
                api.post_empty(
                    &format!("/api/v1/tasks/{key}/runtime"),
                    &json!({ "mode": mode.name() }),
                )?;
                println!("{}", mode.done_line(&key));
            }
            (None, _, None) => unreachable!("runtime_target needs a key or a session"),
        },
        Command::Reopen { key } => {
            let task: Task = api.post(&format!("/api/v1/tasks/{key}/reopen"), &json!({}))?;
            println!("{} is {} again.", task.key, status_label(&task.status));
        }
        Command::Autopilot { key, action } => {
            let task: Task = api.post(
                &format!("/api/v1/tasks/{key}/autopilot"),
                &json!({ "action": action.name() }),
            )?;
            println!("{}: autopilot {}", task.key, autopilot_label(&task));
        }
        Command::Autofix { key, action, repo } => {
            let body = autofix_body(action, repo.as_deref())?;
            let task: Task = api.post(&format!("/api/v1/tasks/{key}/autofix"), &body)?;
            println!(
                "{}: CI autofix {}",
                task.key,
                match action {
                    AutofixAction::Retry => "retrying",
                    _ if task.ci_autofix => "on",
                    _ => "off",
                }
            );
        }
        Command::Done { key } => {
            let task: Task = api.post(&format!("/api/v1/tasks/{key}/done"), &json!({}))?;
            println!(
                "{} is done. Remove its workspace with `delilah archive {key}`.",
                task.key
            );
        }
        Command::Setup(cmd) => {
            let (key, action) = match cmd {
                SetupCommand::Retry { key } => (key, "retry"),
                SetupCommand::Skip { key } => (key, "skip"),
                SetupCommand::Cancel { key } => (key, "cancel"),
            };
            let task: Task =
                api.post(&format!("/api/v1/tasks/{key}/setup/{action}"), &json!({}))?;
            println!("{} is {}.", task.key, status_label(&task.status));
        }
        Command::Archive { key, force } => {
            let query = if force { "?force=true" } else { "" };
            let task: Task =
                api.post(&format!("/api/v1/tasks/{key}/archive{query}"), &json!({}))?;
            println!("Archived the workspace of {}.", task.key);
        }
        Command::Rm { key, force } => {
            let query = if force { "?force=true" } else { "" };
            match api.delete(&format!("/api/v1/tasks/{key}{query}")) {
                Ok(()) => println!("Deleted {key}."),
                Err(e) if e.unpushed => {
                    return Err(format!("{}; use --force to delete it anyway", e.error));
                }
                Err(e) => return Err(e.error),
            }
        }
        Command::Usage => {
            let report: Option<UsageReport> = api.get("/api/v1/usage")?;
            let Some(report) = report else {
                println!("No usage reported yet: it arrives with the first turn of an agent.");
                return Ok(());
            };
            let at = report.at.with_timezone(&Local).format("%H:%M");
            println!("Claude usage (reported at {at} by {}):", report.task);
            for (name, window) in report.rate_limits.as_object().into_iter().flatten() {
                let used = window["used_percentage"].as_f64().unwrap_or(0.0);
                let reset = window["resets_at"]
                    .as_i64()
                    .and_then(|t| DateTime::from_timestamp(t, 0))
                    .map(|t| t.with_timezone(&Local).format("%a %H:%M").to_string())
                    .unwrap_or_else(|| "-".into());
                println!("  {name:<10} {used:>5.0}%   resets {reset}");
            }
        }
        Command::Project(ProjectCommand::Add {
            path,
            kind,
            name,
            workflow,
        }) => {
            // The daemon runs in another folder: send it an absolute path.
            let path =
                std::fs::canonicalize(&path).map_err(|e| format!("{}: {e}", path.display()))?;
            let project: Project = api.post(
                "/api/v1/projects",
                &json!({ "path": path, "kind": kind, "name": name, "workflow": workflow }),
            )?;
            println!(
                "Added {} ({}) at {}",
                project.name,
                project.kind.name(),
                project.path.display()
            );
        }
        Command::Project(ProjectCommand::Show { name }) => {
            let project = find_project(api, &name)?;
            println!("{}  ({})", project.name, project.kind.name());
            for line in settings_lines(&project.settings) {
                println!("  {line}");
            }
        }
        Command::Project(ProjectCommand::Set { name, setting }) => {
            change_project(api, &name, setting.into())?;
        }
        Command::Project(ProjectCommand::Unset { name, setting }) => {
            change_project(api, &name, Change::Unset(setting))?;
        }
        Command::Project(ProjectCommand::Ls) => {
            let projects: Vec<Project> = api.get("/api/v1/projects")?;
            if projects.is_empty() {
                println!(
                    "No projects yet. Add one with `delilah project add <PATH> --kind parent|repo`"
                );
            }
            for p in projects {
                println!(
                    "{:<16} {:<7} {:<10} {}",
                    p.name,
                    p.kind.name(),
                    p.workflow,
                    p.path.display()
                );
            }
        }
    }
    Ok(())
}

fn find_project(api: &Api, name: &str) -> Result<Project, String> {
    let projects: Vec<Project> = api.get("/api/v1/projects")?;
    projects
        .into_iter()
        .find(|p| p.name == name)
        .ok_or_else(|| format!("no such project: {name}"))
}

/// The project came from the list, so a bare 404/405 means the daemon lacks the route.
fn is_missing_route(error: &str) -> bool {
    error == "daemon answered 404 Not Found" || error == "daemon answered 405 Method Not Allowed"
}

/// Reads the project, changes one field and sends all settings back.
fn change_project(api: &Api, name: &str, change: Change) -> Result<(), String> {
    let project = find_project(api, name)?;
    let settings = apply_change(project.settings, change)?;
    let updated: Project = api
        .put(&format!("/api/v1/projects/{name}/settings"), &settings)
        .map_err(|e| {
            if e.fields.is_empty() {
                if is_missing_route(&e.error) {
                    "this delilahd doesn't support project settings; update/restart it".to_string()
                } else {
                    e.error
                }
            } else {
                e.fields
                    .iter()
                    .map(|f| format!("{}: {}", f.field, f.message))
                    .collect::<Vec<_>>()
                    .join("\n")
            }
        })?;
    println!("Updated {}:", updated.name);
    for line in settings_lines(&updated.settings) {
        println!("  {line}");
    }
    Ok(())
}

/// The autopilot of a task as the daemon reports it: "off", "on (running)", "paused: why"…
fn autopilot_label(task: &Task) -> String {
    let Some(a) = &task.autopilot else {
        return "off".into();
    };
    match (a.state, &a.reason) {
        (AutopilotState::Running, _) => "on (running)".into(),
        (AutopilotState::Paused, Some(reason)) => format!("paused ({reason})"),
        (AutopilotState::Paused, None) => "paused".into(),
        (AutopilotState::Done, _) => "on (done)".into(),
    }
}

fn status_label(status: &TaskStatus) -> String {
    match status {
        TaskStatus::PausedByLimit { resume_at } => {
            format!(
                "paused until {}",
                resume_at.with_timezone(&Local).format("%H:%M")
            )
        }
        TaskStatus::Failed { reason } => format!("failed: {reason}"),
        TaskStatus::SetupFailed { reason } => {
            let last = reason
                .lines()
                .rev()
                .map(str::trim)
                .find(|l| !l.is_empty())
                .unwrap_or("");
            format!("setup-failed: {last}")
        }
        TaskStatus::Queued => "backlog".to_string(),
        other => other.name().to_string(),
    }
}

struct Api {
    client: Client,
    base: String,
}

impl Api {
    fn get<T: for<'de> Deserialize<'de>>(&self, path: &str) -> Result<T, String> {
        self.handle(self.client.get(format!("{}{path}", self.base)).send())
    }

    fn post<T: for<'de> Deserialize<'de>>(
        &self,
        path: &str,
        body: &impl serde::Serialize,
    ) -> Result<T, String> {
        self.handle(
            self.client
                .post(format!("{}{path}", self.base))
                .json(body)
                .send(),
        )
    }

    /// A `PUT`; the error keeps the daemon's per-field messages.
    fn put<T: for<'de> Deserialize<'de>>(
        &self,
        path: &str,
        body: &impl serde::Serialize,
    ) -> Result<T, ErrorBody> {
        let res = self
            .client
            .put(format!("{}{path}", self.base))
            .json(body)
            .send()
            .map_err(|e| ErrorBody::from(self.unreachable(e)))?;
        let status = res.status();
        if status.is_success() {
            return res
                .json()
                .map_err(|e| format!("unexpected response: {e}").into());
        }
        Err(res
            .json::<ErrorBody>()
            .unwrap_or_else(|_| format!("daemon answered {status}").into()))
    }

    /// A `POST` answered with no body (204).
    fn post_empty(&self, path: &str, body: &impl serde::Serialize) -> Result<(), String> {
        let res = self
            .client
            .post(format!("{}{path}", self.base))
            .json(body)
            .send()
            .map_err(|e| self.unreachable(e))?;
        let status = res.status();
        if status.is_success() {
            return Ok(());
        }
        match res.json::<ErrorBody>() {
            Ok(body) => Err(body.error),
            Err(_) => Err(format!("daemon answered {status}")),
        }
    }

    /// A `DELETE` answered with no body; the error keeps the daemon's flags.
    fn delete(&self, path: &str) -> Result<(), ErrorBody> {
        let res = self
            .client
            .delete(format!("{}{path}", self.base))
            .send()
            .map_err(|e| ErrorBody::from(self.unreachable(e)))?;
        if res.status().is_success() {
            return Ok(());
        }
        let status = res.status();
        Err(res
            .json::<ErrorBody>()
            .unwrap_or_else(|_| format!("daemon answered {status}").into()))
    }

    fn unreachable(&self, e: reqwest::Error) -> String {
        if e.is_connect() {
            format!(
                "cannot reach delilahd at {}; is it running? (`cargo run -p delilah-daemon`)",
                self.base
            )
        } else {
            e.to_string()
        }
    }

    fn handle<T: for<'de> Deserialize<'de>>(
        &self,
        sent: reqwest::Result<Response>,
    ) -> Result<T, String> {
        let res = sent.map_err(|e| self.unreachable(e))?;
        if res.status().is_success() {
            return res.json().map_err(|e| format!("unexpected response: {e}"));
        }
        let status = res.status();
        match res.json::<ErrorBody>() {
            Ok(body) => Err(body.error),
            Err(_) => Err(format!("daemon answered {status}")),
        }
    }
}

/// `--checkout` / `--worktree` as the request's `in_checkout`; none given means the
/// daemon decides (the backlog task's choice, else the project's setting).
fn choice(checkout: bool, worktree: bool) -> Option<bool> {
    match (checkout, worktree) {
        (true, _) => Some(true),
        (_, true) => Some(false),
        _ => None,
    }
}

/// Sets `in_checkout` only when a choice was made; omitted, the daemon's default applies.
fn with_choice(body: &mut serde_json::Value, in_checkout: Option<bool>) {
    if let Some(c) = in_checkout {
        body["in_checkout"] = json!(c);
    }
}

fn new_body(
    key: String,
    title: String,
    project: String,
    notes: Option<String>,
    free: bool,
    in_checkout: Option<bool>,
    workflow: Option<String>,
) -> serde_json::Value {
    let mut body = json!({
        "key": key, "title": title, "project": project,
        "notes": notes, "free": free, "workflow": workflow,
    });
    with_choice(&mut body, in_checkout);
    body
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flags(args: &[&str]) -> Result<Option<bool>, clap::Error> {
        Ok(match Cli::try_parse_from(args)?.command {
            Command::New {
                checkout, worktree, ..
            }
            | Command::Run {
                checkout, worktree, ..
            } => choice(checkout, worktree),
            _ => panic!("not new/run"),
        })
    }

    fn saving_choice(args: &[&str]) -> Result<Option<bool>, clap::Error> {
        let mut argv = vec!["delilah"];
        argv.extend_from_slice(args);
        Cli::try_parse_from(argv).map(|cli| match cli.command {
            Command::New { saving, .. } | Command::Run { saving, .. } => saving.choice(),
            _ => panic!("not new or run"),
        })
    }

    #[test]
    fn new_and_run_take_an_optional_workspace_choice() {
        for (cmd, rest) in [
            ("new", vec!["K-1", "--title", "t", "--project", "p"]),
            ("run", vec!["K-1"]),
        ] {
            let with = |extra: &[&str]| {
                let mut v = vec!["delilah", cmd];
                v.extend(&rest);
                v.extend(extra);
                flags(&v)
            };
            assert_eq!(with(&[]).unwrap(), None, "{cmd}");
            assert_eq!(with(&["--checkout"]).unwrap(), Some(true), "{cmd}");
            assert_eq!(with(&["--worktree"]).unwrap(), Some(false), "{cmd}");
            assert!(with(&["--checkout", "--worktree"]).is_err(), "{cmd}");
        }
    }

    #[test]
    fn saving_mode_flags_on_new_and_run() {
        let new = ["new", "K-1", "--title", "t", "--project", "p"];
        let run = ["run", "K-1"];
        for base in [&new[..], &run[..]] {
            let with = |flag: &'static str| [base, &[flag]].concat();
            assert_eq!(saving_choice(base).unwrap(), None);
            assert_eq!(saving_choice(&with("--saving-mode")).unwrap(), Some(true));
            assert_eq!(
                saving_choice(&with("--no-saving-mode")).unwrap(),
                Some(false)
            );
            assert!(
                saving_choice(&[base, &["--saving-mode", "--no-saving-mode"]].concat()).is_err()
            );
        }
    }

    #[test]
    fn the_request_omits_in_checkout_without_a_choice() {
        let body = |c| new_body("K".into(), "t".into(), "p".into(), None, false, c, None);
        assert!(body(None).get("in_checkout").is_none());
        assert_eq!(body(Some(true))["in_checkout"], json!(true));
        assert_eq!(body(Some(false))["in_checkout"], json!(false));
        let mut run = json!({});
        with_choice(&mut run, None);
        assert!(run.get("in_checkout").is_none());
    }

    fn parse(args: &[&str]) -> ProjectCommand {
        let mut v = vec!["delilah", "project"];
        v.extend(args);
        match Cli::try_parse_from(v).expect("parses").command {
            Command::Project(p) => p,
            _ => panic!("not a project command"),
        }
    }

    fn change(args: &[&str]) -> Change {
        match parse(args) {
            ProjectCommand::Set { setting, .. } => setting.into(),
            ProjectCommand::Unset { setting, .. } => Change::Unset(setting),
            _ => panic!("not set/unset"),
        }
    }

    #[test]
    fn chat_flag_on_new_and_run_sets_chat_mode() {
        for args in [
            &["new", "K-1", "--title", "t", "--project", "p"][..],
            &["run", "K-1"][..],
        ] {
            let chat = |extra: &[&str]| {
                let mut argv = vec!["delilah"];
                argv.extend_from_slice(args);
                argv.extend_from_slice(extra);
                match Cli::try_parse_from(argv).unwrap().command {
                    Command::New { chat, .. } | Command::Run { chat, .. } => chat,
                    _ => panic!("not new or run"),
                }
            };
            assert!(!chat(&[]));
            assert!(chat(&["--chat"]));
        }
    }

    #[test]
    fn chat_mode_is_sent_only_with_chat() {
        let body = || json!({"key": "K-1"});
        assert!(with_chat(body(), false).get("chat_mode").is_none());
        assert_eq!(with_chat(body(), true)["chat_mode"], true);
    }

    #[test]
    fn saving_mode_is_sent_only_when_chosen() {
        let body = || json!({"key": "K-1"});
        assert!(with_saving_mode(body(), None).get("saving_mode").is_none());
        assert_eq!(with_saving_mode(body(), Some(true))["saving_mode"], true);
        assert_eq!(with_saving_mode(body(), Some(false))["saving_mode"], false);
    }

    fn autopilot_choice(args: &[&str]) -> Result<Option<bool>, clap::Error> {
        let mut argv = vec!["delilah"];
        argv.extend_from_slice(args);
        Cli::try_parse_from(argv).map(|cli| match cli.command {
            Command::New { autopilot, .. } | Command::Run { autopilot, .. } => autopilot.choice(),
            _ => panic!("not new or run"),
        })
    }

    #[test]
    fn autopilot_flags_on_new_and_run() {
        let new = ["new", "K-1", "--title", "t", "--project", "p"];
        let run = ["run", "K-1"];
        for base in [&new[..], &run[..]] {
            let with = |flag: &'static str| [base, &[flag]].concat();
            assert_eq!(autopilot_choice(base).unwrap(), None);
            assert_eq!(autopilot_choice(&with("--autopilot")).unwrap(), Some(true));
            assert_eq!(
                autopilot_choice(&with("--no-autopilot")).unwrap(),
                Some(false)
            );
            assert!(
                autopilot_choice(&[base, &["--autopilot", "--no-autopilot"]].concat()).is_err()
            );
            assert!(
                autopilot_choice(&[base, &["--autopilot", "--no-saving-mode"]].concat()).is_err()
            );
            // Autopilot off is fine without saving mode.
            assert_eq!(
                autopilot_choice(&[base, &["--no-autopilot", "--no-saving-mode"]].concat())
                    .unwrap(),
                Some(false)
            );
        }
    }

    fn autofix_choice(args: &[&str]) -> Result<Option<bool>, clap::Error> {
        let mut argv = vec!["delilah"];
        argv.extend_from_slice(args);
        Cli::try_parse_from(argv).map(|cli| match cli.command {
            Command::New { autofix, .. } | Command::Run { autofix, .. } => autofix.choice(),
            _ => panic!("not new or run"),
        })
    }

    #[test]
    fn autofix_flags_on_new_and_run() {
        let new = ["new", "K-1", "--title", "t", "--project", "p"];
        let run = ["run", "K-1"];
        for base in [&new[..], &run[..]] {
            let with = |flag: &'static str| [base, &[flag]].concat();
            assert_eq!(autofix_choice(base).unwrap(), None);
            assert_eq!(autofix_choice(&with("--autofix")).unwrap(), Some(true));
            assert_eq!(autofix_choice(&with("--no-autofix")).unwrap(), Some(false));
            assert!(autofix_choice(&[base, &["--autofix", "--no-autofix"]].concat()).is_err());
        }
    }

    #[test]
    fn ci_autofix_is_sent_only_when_chosen() {
        let body = || json!({"key": "K-1"});
        assert!(with_ci_autofix(body(), None).get("ci_autofix").is_none());
        assert_eq!(with_ci_autofix(body(), Some(true))["ci_autofix"], true);
        assert_eq!(with_ci_autofix(body(), Some(false))["ci_autofix"], false);
    }

    #[test]
    fn autofix_subcommand_takes_on_off_retry() {
        for word in ["on", "off", "retry"] {
            let cli =
                Cli::try_parse_from(["delilah", "autofix", "K-1", word, "--repo", "."]).unwrap();
            match cli.command {
                Command::Autofix { key, action, repo } => {
                    assert_eq!(key, "K-1");
                    assert_eq!(action.name(), word);
                    assert_eq!(repo.as_deref(), Some("."));
                }
                _ => panic!("not autofix"),
            }
        }
        assert!(Cli::try_parse_from(["delilah", "autofix", "K-1", "maybe"]).is_err());
        assert!(Cli::try_parse_from(["delilah", "autofix", "K-1"]).is_err());
    }

    #[test]
    fn autofix_bodies() {
        assert_eq!(
            autofix_body(AutofixAction::On, None).unwrap(),
            json!({"action": "on"})
        );
        assert_eq!(
            autofix_body(AutofixAction::Off, None).unwrap(),
            json!({"action": "off"})
        );
        assert_eq!(
            autofix_body(AutofixAction::Retry, Some(".")).unwrap(),
            json!({"action": "retry", "repo": "."})
        );
        assert!(autofix_body(AutofixAction::Retry, None).is_err());
    }

    #[test]
    fn ci_autofix_project_setting() {
        for (word, want) in [
            ("off", AutofixDefault::Off),
            ("autopilot", AutofixDefault::Autopilot),
            ("always", AutofixDefault::Always),
        ] {
            let c = change(&["set", "p", "ci-autofix", word]);
            assert_eq!(
                c,
                Change::CiAutofix(match want {
                    AutofixDefault::Off => AutofixArg::Off,
                    AutofixDefault::Autopilot => AutofixArg::Autopilot,
                    AutofixDefault::Always => AutofixArg::Always,
                })
            );
            let s = apply_change(ProjectSettings::default(), c).unwrap();
            assert_eq!(s.ci_autofix, want);
        }
        assert!(
            Cli::try_parse_from(["delilah", "project", "set", "p", "ci-autofix", "x"]).is_err()
        );
        let always = apply_change(
            ProjectSettings::default(),
            Change::CiAutofix(AutofixArg::Always),
        )
        .unwrap();
        assert!(
            settings_lines(&always)
                .iter()
                .any(|l| l.starts_with("ci-autofix: always"))
        );
        let back = apply_change(always, change(&["unset", "p", "ci-autofix"])).unwrap();
        assert_eq!(back.ci_autofix, AutofixDefault::Autopilot);
    }

    #[test]
    fn parses_setup_commands() {
        for (word, ok) in [("retry", 0), ("skip", 1), ("cancel", 2)] {
            let cli = Cli::try_parse_from(["delilah", "setup", word, "K-1"]).expect("parses");
            let hit = match cli.command {
                Command::Setup(SetupCommand::Retry { key }) => (0, key),
                Command::Setup(SetupCommand::Skip { key }) => (1, key),
                Command::Setup(SetupCommand::Cancel { key }) => (2, key),
                _ => panic!("not a setup command"),
            };
            assert_eq!(hit, (ok, "K-1".to_string()));
        }
    }

    #[test]
    fn setup_failed_label_shows_last_line_of_reason() {
        let status = TaskStatus::SetupFailed {
            reason: "first\nboom\n\n".into(),
        };
        assert_eq!(status_label(&status), "setup-failed: boom");
    }

    #[test]
    fn parses_show() {
        assert!(matches!(parse(&["show", "p"]), ProjectCommand::Show { name } if name == "p"));
    }

    #[test]
    fn detects_a_daemon_without_the_settings_route() {
        assert!(is_missing_route("daemon answered 404 Not Found"));
        assert!(is_missing_route("daemon answered 405 Method Not Allowed"));
        assert!(!is_missing_route("no such project: x"));
    }

    #[test]
    fn parses_scalar_settings() {
        assert_eq!(
            change(&["set", "p", "workspace", "worktree"]),
            Change::Workspace(WorkspaceArg::Worktree)
        );
        assert_eq!(
            change(&["set", "p", "branch", "feature/{ticket}-{slug}"]),
            Change::Branch("feature/{ticket}-{slug}".into())
        );
        assert_eq!(
            change(&["set", "p", "setup", "npm ci"]),
            Change::Setup("npm ci".into())
        );
        assert!(Cli::try_parse_from(["delilah", "project", "set", "p", "workspace", "x"]).is_err());
    }

    #[test]
    fn parses_shared() {
        assert_eq!(
            change(&["set", "p", "shared", "add", ".env"]),
            Change::SharedAdd(".env".into(), None)
        );
        assert_eq!(
            change(&["set", "p", "shared", "add", ".env", "--copy"]),
            Change::SharedAdd(".env".into(), Some(ShareMode::Copy))
        );
        assert_eq!(
            change(&["set", "p", "shared", "add", "node_modules", "--link"]),
            Change::SharedAdd("node_modules".into(), Some(ShareMode::Link))
        );
        assert_eq!(
            change(&["set", "p", "shared", "rm", ".env"]),
            Change::SharedRm(".env".into())
        );
        assert!(
            Cli::try_parse_from([
                "delilah", "project", "set", "p", "shared", "add", "x", "--copy", "--link"
            ])
            .is_err()
        );
    }

    #[test]
    fn parses_submodules_and_unset() {
        assert_eq!(
            change(&["set", "p", "submodules", "always", "add", "api"]),
            Change::SubmodulesAdd(SubmoduleRule::Always, "api".into())
        );
        assert_eq!(
            change(&["set", "p", "submodules", "never", "rm", "web"]),
            Change::SubmodulesRm(SubmoduleRule::Never, "web".into())
        );
        assert_eq!(
            change(&["unset", "p", "branch"]),
            Change::Unset(SettingName::Branch)
        );
        assert!(Cli::try_parse_from(["delilah", "project", "unset", "p", "nope"]).is_err());
    }

    #[test]
    fn applies_changes() {
        let d = ProjectSettings::default();
        let s = apply_change(d.clone(), Change::Workspace(WorkspaceArg::Worktree)).unwrap();
        assert_eq!(s.workspace, Some(WorkspaceMode::Worktree));
        let s = apply_change(s, Change::SharedAdd(".env".into(), Some(ShareMode::Copy))).unwrap();
        let s = apply_change(s, Change::SharedAdd(".env".into(), Some(ShareMode::Link))).unwrap();
        assert_eq!(s.shared.len(), 1);
        assert_eq!(s.shared[0].mode, Some(ShareMode::Link));
        assert!(apply_change(s.clone(), Change::SharedRm("nope".into())).is_err());
        let s = apply_change(s, Change::SharedRm(".env".into())).unwrap();
        assert!(s.shared.is_empty());
        let s = apply_change(s, Change::SubmodulesAdd(SubmoduleRule::Never, "a".into())).unwrap();
        let s = apply_change(s, Change::SubmodulesAdd(SubmoduleRule::Never, "a".into())).unwrap();
        assert_eq!(s.submodules.never, vec!["a".to_string()]);
        let s = apply_change(s, Change::SubmodulesRm(SubmoduleRule::Never, "a".into())).unwrap();
        assert!(s.submodules.never.is_empty());
        assert!(apply_change(s, Change::SubmodulesRm(SubmoduleRule::Always, "a".into())).is_err());
    }

    #[test]
    fn unset_returns_to_default() {
        let mut s = ProjectSettings {
            workspace: Some(WorkspaceMode::Worktree),
            branch: Some("x/{ticket}".into()),
            setup: Some("make".into()),
            ..Default::default()
        };
        s.shared.push(SharedEntry {
            path: "a".into(),
            mode: None,
        });
        s.submodules.always.push("m".into());
        for name in [
            SettingName::Workspace,
            SettingName::Branch,
            SettingName::Setup,
            SettingName::Shared,
            SettingName::Submodules,
        ] {
            s = apply_change(s, Change::Unset(name)).unwrap();
        }
        assert_eq!(s, ProjectSettings::default());
    }

    #[test]
    fn show_says_who_names_the_branch_by_default() {
        let mut s = ProjectSettings::default();
        assert!(settings_lines(&s)[1].contains("(default: proposed by Delilah)"));
        s.branch = Some("fix/{ticket}".into());
        assert!(settings_lines(&s)[1].contains("fix/{ticket}"));
    }

    #[test]
    fn show_marks_defaults() {
        let mut s = ProjectSettings::default();
        assert!(settings_lines(&s)[0].ends_with("(default)"));
        assert!(settings_lines(&s)[0].starts_with("workspace:  checkout"));
        s.workspace = Some(WorkspaceMode::Worktree);
        assert!(!settings_lines(&s)[0].contains("(default)"));
        s.workspace = Some(WorkspaceMode::Checkout);
        assert!(settings_lines(&s)[0].contains("checkout"));
        assert!(!settings_lines(&s)[0].contains("(default)"));
    }

    #[test]
    fn autopilot_is_sent_only_when_chosen() {
        let body = || json!({"key": "K-1"});
        assert!(with_autopilot(body(), None).get("autopilot").is_none());
        assert_eq!(with_autopilot(body(), Some(true))["autopilot"], true);
        assert_eq!(with_autopilot(body(), Some(false))["autopilot"], false);
    }

    #[test]
    fn autopilot_label_reads_the_returned_state() {
        use delilah_core::Autopilot;
        let mut task = Task::new(delilah_core::TaskKey::new("K-1"), "t", "feature");
        assert_eq!(autopilot_label(&task), "off");
        let mut a = Autopilot {
            state: AutopilotState::Running,
            stage: None,
            reason: None,
            since: Utc::now(),
            nudged: None,
            review_session: None,
        };
        task.autopilot = Some(a.clone());
        assert_eq!(autopilot_label(&task), "on (running)");
        a.state = AutopilotState::Paused;
        task.autopilot = Some(a.clone());
        assert_eq!(autopilot_label(&task), "paused");
        a.reason = Some("a question".into());
        task.autopilot = Some(a);
        assert_eq!(autopilot_label(&task), "paused (a question)");
    }

    #[test]
    fn autopilot_subcommand_takes_on_off_resume() {
        for word in ["on", "off", "resume"] {
            let cli = Cli::try_parse_from(["delilah", "autopilot", "K-1", word]).unwrap();
            match cli.command {
                Command::Autopilot { key, action } => {
                    assert_eq!(key, "K-1");
                    assert_eq!(action.name(), word);
                }
                _ => panic!("not autopilot"),
            }
        }
        assert!(Cli::try_parse_from(["delilah", "autopilot", "K-1", "maybe"]).is_err());
        assert!(Cli::try_parse_from(["delilah", "autopilot", "K-1"]).is_err());
    }

    #[test]
    fn runtime_subcommand_takes_chat_or_terminal() {
        for word in ["chat", "terminal"] {
            let cli = Cli::try_parse_from(["delilah", "runtime", "K-1", word]).unwrap();
            match cli.command {
                Command::Runtime { words, session } => {
                    let (key, mode, id) = runtime_target(&words, session.as_deref()).unwrap();
                    assert_eq!(key.as_deref(), Some("K-1"));
                    assert_eq!(mode.name(), word);
                    assert!(id.is_none());
                }
                _ => panic!("not runtime"),
            }
        }
        let gui = Cli::try_parse_from(["delilah", "runtime", "K-1", "gui"]).unwrap();
        match gui.command {
            Command::Runtime { words, session } => {
                assert!(runtime_target(&words, session.as_deref()).is_err())
            }
            _ => panic!("not runtime"),
        }
        let one = Cli::try_parse_from(["delilah", "runtime", "K-1"]).unwrap();
        match one.command {
            Command::Runtime { words, session } => {
                assert!(runtime_target(&words, session.as_deref()).is_err())
            }
            _ => panic!("not runtime"),
        }
        assert_eq!(
            RuntimeMode::Chat.done_line("K-1"),
            "K-1 now runs in chat mode."
        );
        assert_eq!(
            RuntimeMode::Terminal.done_line("K-1"),
            "K-1 now runs in the terminal."
        );
    }

    #[test]
    fn runtime_prints_the_409_message_and_accepts_204() {
        use std::io::{Read, Write};
        use std::net::TcpListener;
        fn serve(reply: &'static str) -> String {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let base = format!("http://{}", listener.local_addr().unwrap());
            std::thread::spawn(move || {
                let (mut s, _) = listener.accept().unwrap();
                let mut buf = [0u8; 4096];
                let _ = s.read(&mut buf);
                s.write_all(reply.as_bytes()).unwrap();
            });
            base
        }
        let api = |base| Api {
            client: Client::new(),
            base,
        };
        let msg = "wait for the turn to end or press Detener";
        let body = format!("{{\"error\":\"{msg}\"}}");
        let reply: &'static str = Box::leak(
            format!(
                "HTTP/1.1 409 Conflict\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                body.len()
            )
            .into_boxed_str(),
        );
        let err = run(
            &api(serve(reply)),
            Cli::try_parse_from(["delilah", "runtime", "K-1", "chat"])
                .unwrap()
                .command,
        )
        .unwrap_err();
        assert_eq!(err, msg);
        let ok = run(
            &api(serve(
                "HTTP/1.1 204 No Content\r\nconnection: close\r\n\r\n",
            )),
            Cli::try_parse_from(["delilah", "runtime", "K-1", "terminal"])
                .unwrap()
                .command,
        );
        assert!(ok.is_ok());
    }

    #[test]
    fn runtime_session_form_and_reopen_parse() {
        let cli = Cli::try_parse_from(["delilah", "runtime", "--session", "S-1", "chat"]).unwrap();
        match cli.command {
            Command::Runtime { words, session } => {
                let (key, mode, id) = runtime_target(&words, session.as_deref()).unwrap();
                assert!(key.is_none());
                assert_eq!(mode.name(), "chat");
                assert_eq!(id.as_deref(), Some("S-1"));
            }
            _ => panic!("not runtime"),
        }
        assert!(matches!(
            Cli::try_parse_from(["delilah", "reopen", "K-1"]).unwrap().command,
            Command::Reopen { key } if key == "K-1"
        ));
        assert!(Cli::try_parse_from(["delilah", "reopen"]).is_err());
    }

    #[test]
    fn session_runtime_and_reopen_show_the_daemon_messages() {
        use std::io::{Read, Write};
        use std::net::TcpListener;
        fn serve(reply: String) -> (String, std::thread::JoinHandle<String>) {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let base = format!("http://{}", listener.local_addr().unwrap());
            let h = std::thread::spawn(move || {
                let (mut s, _) = listener.accept().unwrap();
                let mut buf = [0u8; 4096];
                let n = s.read(&mut buf).unwrap();
                s.write_all(reply.as_bytes()).unwrap();
                String::from_utf8_lossy(&buf[..n]).to_string()
            });
            (base, h)
        }
        let conflict = |msg: &str| {
            let body = format!("{{\"error\":\"{msg}\"}}");
            format!(
                "HTTP/1.1 409 Conflict\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                body.len()
            )
        };
        let api = |base| Api {
            client: Client::new(),
            base,
        };
        let (base, h) = serve(conflict("already runs in chat"));
        let err = run(
            &api(base),
            Cli::try_parse_from(["delilah", "runtime", "--session", "S-1", "chat"])
                .unwrap()
                .command,
        )
        .unwrap_err();
        assert_eq!(err, "already runs in chat");
        assert!(
            h.join()
                .unwrap()
                .starts_with("POST /api/v1/sessions/S-1/runtime")
        );
        let (base, h) = serve("HTTP/1.1 204 No Content\r\nconnection: close\r\n\r\n".into());
        assert!(
            run(
                &api(base),
                Cli::try_parse_from(["delilah", "runtime", "--session", "S-1", "terminal"])
                    .unwrap()
                    .command,
            )
            .is_ok()
        );
        h.join().unwrap();
        let (base, h) = serve(conflict("task is not done"));
        let err = run(
            &api(base),
            Cli::try_parse_from(["delilah", "reopen", "K-1"])
                .unwrap()
                .command,
        )
        .unwrap_err();
        assert_eq!(err, "task is not done");
        assert!(
            h.join()
                .unwrap()
                .starts_with("POST /api/v1/tasks/K-1/reopen")
        );
    }
}
