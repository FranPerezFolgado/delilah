//! `delilah`: a client of the daemon's HTTP API. It never touches the database.

mod attach;

use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;

use chrono::{DateTime, Local, Utc};
use clap::{Args, Parser, Subcommand};
use delilah_core::{Event, Project, Task, TaskStatus};
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
    /// `None` when no flag was given: the daemon then reads the preference.
    fn choice(&self) -> Option<bool> {
        match (self.saving_mode, self.no_saving_mode) {
            (true, _) => Some(true),
            (_, true) => Some(false),
            _ => None,
        }
    }
}

/// Adds `saving_mode` to a request body only when the user chose it.
fn with_saving_mode(mut body: serde_json::Value, choice: Option<bool>) -> serde_json::Value {
    if let Some(on) = choice {
        body["saving_mode"] = json!(on);
    }
    body
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
        /// Work in the project's checkout once started (by default, in a worktree).
        #[arg(long)]
        checkout: bool,
        /// Defaults to the project's workflow.
        #[arg(long)]
        workflow: Option<String>,
        #[command(flatten)]
        saving: SavingModeFlags,
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
        /// Defaults to feature/<KEY>.
        #[arg(long)]
        branch: Option<String>,
        /// First prompt; defaults to `/feature <KEY>`.
        #[arg(long)]
        prompt: Option<String>,
        /// Attach to the agent's terminal right away.
        #[arg(long)]
        attach: bool,
        #[command(flatten)]
        saving: SavingModeFlags,
    },
    /// Opens the live terminal of a task's agent (Ctrl-] detaches).
    Attach { key: String },
    /// Resumes a task paused by the usage limit now, without waiting for the reset.
    Resume { key: String },
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
}

#[derive(Deserialize)]
struct Health {
    version: String,
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
}

impl From<String> for ErrorBody {
    fn from(error: String) -> Self {
        Self {
            error,
            unpushed: false,
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
            workflow,
            saving,
        } => {
            if key.is_none() && !free {
                return Err("say the ticket's KEY, or --free for a task without one".into());
            }
            let task: Task = api.post(
                "/api/v1/tasks",
                &with_saving_mode(
                    json!({
                        "key": key.unwrap_or_default(), "title": title, "project": project,
                        "notes": notes, "free": free, "in_checkout": checkout, "workflow": workflow,
                    }),
                    saving.choice(),
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
            attach,
            saving,
        } => {
            let task: Task = api.post(
                "/api/v1/runs",
                &with_saving_mode(
                    json!({
                        "key": key, "title": title.unwrap_or_default(), "project": project.unwrap_or_default(),
                        "workflow": workflow,
                        "submodules": submodules, "branch": branch, "prompt": prompt,
                    }),
                    saving.choice(),
                ),
            )?;
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
        Command::Done { key } => {
            let task: Task = api.post(&format!("/api/v1/tasks/{key}/done"), &json!({}))?;
            println!(
                "{} is done. Remove its workspace with `delilah archive {key}`.",
                task.key
            );
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

fn status_label(status: &TaskStatus) -> String {
    match status {
        TaskStatus::PausedByLimit { resume_at } => {
            format!(
                "paused until {}",
                resume_at.with_timezone(&Local).format("%H:%M")
            )
        }
        TaskStatus::Failed { reason } => format!("failed: {reason}"),
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

#[cfg(test)]
mod tests {
    use super::*;

    fn choice(args: &[&str]) -> Result<Option<bool>, clap::Error> {
        let mut argv = vec!["delilah"];
        argv.extend_from_slice(args);
        Cli::try_parse_from(argv).map(|cli| match cli.command {
            Command::New { saving, .. } | Command::Run { saving, .. } => saving.choice(),
            _ => panic!("not new or run"),
        })
    }

    #[test]
    fn saving_mode_flags_on_new_and_run() {
        let new = ["new", "K-1", "--title", "t", "--project", "p"];
        let run = ["run", "K-1"];
        for base in [&new[..], &run[..]] {
            let with = |flag: &'static str| [base, &[flag]].concat();
            assert_eq!(choice(base).unwrap(), None);
            assert_eq!(choice(&with("--saving-mode")).unwrap(), Some(true));
            assert_eq!(choice(&with("--no-saving-mode")).unwrap(), Some(false));
            assert!(choice(&[base, &["--saving-mode", "--no-saving-mode"]].concat()).is_err());
        }
    }

    #[test]
    fn saving_mode_is_sent_only_when_chosen() {
        let body = || json!({"key": "K-1"});
        assert!(with_saving_mode(body(), None).get("saving_mode").is_none());
        assert_eq!(with_saving_mode(body(), Some(true))["saving_mode"], true);
        assert_eq!(with_saving_mode(body(), Some(false))["saving_mode"], false);
    }
}
