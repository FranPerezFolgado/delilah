//! A project's setup command: run once in a new worktree, after the shared entries
//! and before the agent starts. It runs as a read-only `Setup` session (its log is
//! the full output); when it ends the agent is launched, or the task stops in
//! `setup-failed` with the output's tail, to be retried, skipped or left alone.

use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use delilah_agents::pty::PtySession;
use delilah_core::{Event, TaskKey, TaskStatus};

use crate::events::Topic;
use crate::run::{AgentLaunch, LaunchError, forget_launch, load_launch, save_launch, start_agent};
use crate::state::AppState;
use crate::store_sessions::{SessionKind, SessionRecord};

/// How long the setup may run before it is stopped and counted as failed.
const TIMEOUT: Duration = Duration::from_secs(600);
/// How much of the output's end is kept on the failed task.
const TAIL_LINES: usize = 20;
/// How much of the log's end is looked at to find those lines.
const TAIL_BYTES: u64 = 256 * 1024;
/// After a kill, how long to wait for the output to drain before moving on.
const KILL_GRACE: Duration = Duration::from_secs(5);

/// Stops the setup and everything it started: the shell leads its own process group
/// under the PTY, so the group gets TERM, then KILL after the grace, and the shell
/// alone is killed if the group could not be signalled.
async fn stop(pty: &PtySession) {
    let Some(pgid) = pty.process_id() else {
        let _ = pty.kill();
        return;
    };
    let signal = |name: &str| {
        std::process::Command::new("kill")
            .args(["-s", name, "--", &format!("-{pgid}")])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .is_ok_and(|s| s.success())
    };
    let termed = signal("TERM");
    let _ = tokio::time::timeout(KILL_GRACE, pty.wait()).await;
    // The shell may be gone while a child ignores TERM: the group still goes.
    let killed = signal("KILL");
    if !termed && !killed {
        let _ = pty.kill();
    }
}

/// Starts the project's setup for a task in `preparing-workspace` whose worktree is
/// saved, and returns at once: the rest happens when the command ends.
pub(crate) async fn start(
    app: &AppState,
    key: &TaskKey,
    command: &str,
    launch: AgentLaunch,
) -> Result<(), LaunchError> {
    save_launch(app, key, &launch)?;
    spawn_setup(app, key, command, TIMEOUT).await
}

/// The workspace to run in and the launch to make afterwards, or why the task cannot
/// be retried or skipped: checked before it leaves `setup-failed`, so a refusal
/// never strands it in `preparing-workspace`.
async fn preflight(
    app: &AppState,
    key: &TaskKey,
) -> Result<(delilah_git::Workspace, AgentLaunch), LaunchError> {
    let (parent, ws) = app
        .store
        .workspace(key)
        .await?
        .ok_or(LaunchError::Unsupported("the task has no workspace"))?;
    // The setup belongs in a new worktree, never in the project's own checkout.
    if ws.path == parent {
        return Err(LaunchError::Unsupported(
            "the setup never runs in the project's checkout",
        ));
    }
    Ok((ws, load_launch(app, key).await?))
}

/// "Reintentar setup": runs it again.
pub(crate) async fn retry(app: &AppState, key: &TaskKey) -> Result<(), LaunchError> {
    let task = app
        .store
        .get_task(key)
        .await?
        .ok_or(crate::store::StoreError::NotFound(key.clone()))?;
    let command = match task.project {
        Some(name) => app.store.project(&name).await?.settings.setup,
        None => None,
    }
    .ok_or(LaunchError::Unsupported("the project has no setup command"))?;
    preflight(app, key).await?;
    app.transition(key, Event::RetrySetup).await?;
    if let Err(e) = spawn_setup(app, key, &command, TIMEOUT).await {
        let reason = format!("cannot start the setup: {e}");
        app.transition(key, Event::SetupFailed { reason }).await?;
        return Err(e);
    }
    Ok(())
}

/// "Lanzar sin setup": launches the agent with the worktree as it is.
pub(crate) async fn skip(app: &AppState, key: &TaskKey) -> Result<(), LaunchError> {
    preflight(app, key).await?;
    app.transition(key, Event::SkipSetup).await?;
    ready_and_launch(app, key).await
}

/// Cancels a running setup; it ends as a failure ("cancelled"), so retry and skip
/// are offered.
pub(crate) async fn cancel(app: &AppState, key: &TaskKey) -> Result<(), LaunchError> {
    let live = app
        .sessions
        .of_task(key.as_str())
        .into_iter()
        .find(|l| l.kind == SessionKind::Setup && !l.pty.has_exited())
        .ok_or(LaunchError::Unsupported("no setup is running"))?;
    app.sessions.mark_cancelled(&live.id);
    stop(&live.pty).await;
    Ok(())
}

/// At daemon start: a task still preparing its workspace has no setup running (they
/// died with the previous daemon), so it stops as `setup-failed` ("interrupted").
pub async fn recover_interrupted(app: &AppState) -> Result<(), LaunchError> {
    for task in app.store.list_tasks().await? {
        if task.status != TaskStatus::PreparingWorkspace {
            continue;
        }
        let running = app
            .sessions
            .of_task(task.key.as_str())
            .iter()
            .any(|l| l.kind == SessionKind::Setup && !l.pty.has_exited());
        if running {
            continue;
        }
        for s in app.store.open_sessions(Some(task.key.as_str())).await? {
            if s.kind == SessionKind::Setup {
                app.store.close_session(&s.id).await?;
            }
        }
        app.transition(
            &task.key,
            Event::SetupFailed {
                reason: "interrupted".into(),
            },
        )
        .await?;
    }
    app.emit(Topic::Sessions);
    Ok(())
}

async fn spawn_setup(
    app: &AppState,
    key: &TaskKey,
    command: &str,
    timeout: Duration,
) -> Result<(), LaunchError> {
    let (parent, ws) = app
        .store
        .workspace(key)
        .await?
        .ok_or(LaunchError::Unsupported("the task has no workspace"))?;
    // The setup belongs in a new worktree, never in the project's own checkout.
    if ws.path == parent {
        return Err(LaunchError::Unsupported(
            "the setup never runs in the project's checkout",
        ));
    }
    let dir = app.config.task_dir(key.as_str());
    std::fs::create_dir_all(&dir)
        .map_err(|e| LaunchError::Start(format!("cannot create {}: {e}", dir.display())))?;

    let session = SessionRecord {
        id: uuid::Uuid::new_v4().to_string(),
        task: key.to_string(),
        kind: SessionKind::Setup,
        label: "Setup".into(),
        primary: false,
        closed: false,
        read_only: true,
    };
    app.store.create_session(&session).await?;
    let log = dir.join(format!("{}-{}.log", session.kind.name(), session.id));

    // Exactly `$SHELL -lc <setup>`: the command is one argument, never part of
    // another shell string; same clean environment as the agents.
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".into());
    let mut cmd = portable_pty::CommandBuilder::new(shell);
    cmd.args(["-lc", command]);
    cmd.env_clear();
    for (name, value) in delilah_agents::env::agent_env() {
        cmd.env(name, value);
    }
    cmd.env("TERM", "xterm-256color");
    cmd.cwd(&ws.path);
    let pty = match PtySession::spawn(cmd, 120, 40, Some(log.clone())) {
        Ok(pty) => pty,
        Err(e) => {
            app.store.close_session(&session.id).await?;
            app.transition(
                key,
                Event::SetupFailed {
                    reason: format!("cannot start the setup: {e}"),
                },
            )
            .await?;
            return Ok(());
        }
    };
    app.sessions.insert(
        &session.id,
        key.as_str(),
        SessionKind::Setup,
        false,
        Arc::clone(&pty),
    );
    tracing::info!(task = %key, session = %session.id, "setup started");

    let app = app.clone();
    let key = key.clone();
    tokio::spawn(async move {
        if let Err(e) = finish(&app, &key, &session.id, &pty, &log, timeout).await {
            tracing::error!(task = %key, "setup: {e}");
        }
    });
    Ok(())
}

/// Waits for the command and acts on how it ended.
async fn finish(
    app: &AppState,
    key: &TaskKey,
    session_id: &str,
    pty: &Arc<PtySession>,
    log: &Path,
    timeout: Duration,
) -> Result<(), LaunchError> {
    let timed_out = tokio::time::timeout(timeout, pty.wait()).await.is_err();
    if timed_out {
        stop(pty).await;
    }
    app.sessions.remove_if_same(session_id, pty);
    app.store.close_session(session_id).await?;
    app.emit(Topic::Sessions);

    let cancelled = app.sessions.take_cancelled(session_id);
    let tail = output_tail(log);
    let failure = if cancelled {
        Some("cancelled".to_string())
    } else if timed_out {
        let note = app.lang().pick(
            "el setup superó el límite de tiempo",
            "the setup exceeded the time limit",
        );
        Some(format!("{note} ({})\n{tail}", describe(timeout)))
    } else {
        match pty.exit_code() {
            Some(0) => None,
            Some(code) if tail.is_empty() => Some(format!("setup exited with code {code}")),
            _ => Some(tail),
        }
    };
    match failure {
        None => ready_and_launch(app, key).await,
        Some(reason) => {
            tracing::info!(task = %key, "setup failed");
            app.transition(key, Event::SetupFailed { reason }).await?;
            Ok(())
        }
    }
}

/// The task is ready: it runs, and its agent starts from the saved launch. If the
/// inputs are gone the task stops in `setup-failed` rather than stay preparing.
async fn ready_and_launch(app: &AppState, key: &TaskKey) -> Result<(), LaunchError> {
    let (ws, launch) = match preflight(app, key).await {
        Ok(inputs) => inputs,
        Err(e) => {
            let reason = format!("cannot launch the agent: {e}");
            app.transition(key, Event::SetupFailed { reason }).await?;
            return Err(e);
        }
    };
    app.transition(key, Event::WorkspaceReady).await?;
    match start_agent(app, key, &ws, launch).await {
        Ok(()) => {
            forget_launch(app, key);
            Ok(())
        }
        Err(e) => {
            app.transition(
                key,
                Event::Fail {
                    reason: format!("agent: {e}"),
                },
            )
            .await?;
            Err(e)
        }
    }
}

fn describe(limit: Duration) -> String {
    match limit.as_secs() {
        s if s >= 60 && s % 60 == 0 => format!("{} min", s / 60),
        s => format!("{s} s"),
    }
}

/// The last lines of the setup's output, as text: the log holds terminal bytes.
fn output_tail(log: &Path) -> String {
    let Ok(mut file) = std::fs::File::open(log) else {
        return String::new();
    };
    let len = file.metadata().map(|m| m.len()).unwrap_or(0);
    let mut bytes = Vec::new();
    if file
        .seek(SeekFrom::Start(len.saturating_sub(TAIL_BYTES)))
        .is_err()
        || file.read_to_end(&mut bytes).is_err()
    {
        return String::new();
    }
    let text = strip_escapes(&String::from_utf8_lossy(&bytes));
    let lines: Vec<&str> = text
        .lines()
        // A progress line rewritten with `\r` shows what it ended as.
        .map(|l| l.rsplit('\r').find(|p| !p.is_empty()).unwrap_or(""))
        .map(str::trim_end)
        .collect();
    let lines: Vec<&str> = lines.iter().copied().skip_while(|l| l.is_empty()).collect();
    let start = lines.len().saturating_sub(TAIL_LINES);
    lines[start..].join("\n").trim_end().to_string()
}

/// Drops colour codes and other escape sequences.
fn strip_escapes(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\u{1b}' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('[') => {
                for n in chars.by_ref() {
                    if ('@'..='~').contains(&n) {
                        break;
                    }
                }
            }
            Some(']') => {
                while let Some(n) = chars.next() {
                    if n == '\u{7}' || (n == '\u{1b}' && chars.next() == Some('\\')) {
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
    use delilah_core::{Project, ProjectKind, Task};
    use delilah_git::Workspace;

    use super::*;
    use crate::config::Config;
    use crate::run::LAUNCH_FILE;
    use crate::store::Store;

    struct Env {
        app: AppState,
        key: TaskKey,
        worktree: std::path::PathBuf,
    }

    async fn env(setup: &str) -> Env {
        let home = std::env::temp_dir().join(format!("delilah-setup-{}", uuid::Uuid::new_v4()));
        let worktree = home.join("workspaces/PROJ-1");
        let parent = home.join("web");
        std::fs::create_dir_all(&worktree).unwrap();
        std::fs::create_dir_all(&parent).unwrap();
        let config = Config {
            home,
            addr: crate::config::DEFAULT_ADDR.parse().unwrap(),
            allow_file_protocol: false,
            notify: false,
            ui_origin: None,
        };
        let app = AppState::new(Store::in_memory().await.unwrap(), config);
        let settings = delilah_core::ProjectSettings {
            setup: Some(setup.into()),
            ..Default::default()
        };
        app.store
            .create_project(&Project {
                name: "web".into(),
                path: parent.clone(),
                kind: ProjectKind::Repo,
                workflow: "feature".into(),
                settings,
            })
            .await
            .unwrap();
        let key = TaskKey::new("PROJ-1");
        let mut task = Task::new(key.clone(), "Retry", "feature");
        task.project = Some("web".into());
        app.store.create_task(&task).await.unwrap();
        app.transition(&key, Event::Start).await.unwrap();
        app.store
            .save_workspace(
                &key,
                &parent,
                &Workspace {
                    path: worktree.clone(),
                    branch: "feature/PROJ-1".into(),
                    kind: ProjectKind::Repo,
                    submodules: vec![],
                },
            )
            .await
            .unwrap();
        Env { app, key, worktree }
    }

    fn launch() -> AgentLaunch {
        AgentLaunch {
            free: true,
            prompt: "/feature PROJ-1".into(),
            custom_prompt: false,
            notes: None,
            adopted: None,
        }
    }

    async fn status(e: &Env) -> TaskStatus {
        e.app.store.get_task(&e.key).await.unwrap().unwrap().status
    }

    async fn wait_for(e: &Env, done: impl Fn(&TaskStatus) -> bool) -> TaskStatus {
        for _ in 0..200 {
            let s = status(e).await;
            if done(&s) {
                return s;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        panic!("task stayed {:?}", status(e).await);
    }

    async fn agent_sessions(e: &Env) -> usize {
        e.app
            .store
            .open_sessions(Some("PROJ-1"))
            .await
            .unwrap()
            .iter()
            .filter(|s| s.kind == SessionKind::Claude)
            .count()
    }

    fn failed_reason(s: TaskStatus) -> String {
        match s {
            TaskStatus::SetupFailed { reason } => reason,
            other => panic!("not setup-failed: {other:?}"),
        }
    }

    #[tokio::test]
    async fn exit_zero_runs_the_task_and_launches_the_agent() {
        let e = env("echo hello; pwd").await;
        start(&e.app, &e.key, "echo hello; pwd", launch())
            .await
            .unwrap();
        wait_for(&e, |s| *s == TaskStatus::Running).await;
        assert_eq!(agent_sessions(&e).await, 1);
        let dir = e.app.config.task_dir("PROJ-1");
        assert!(!dir.join(LAUNCH_FILE).exists());
        // The full output stays with the task's logs; the session is closed.
        let log = std::fs::read_dir(&dir)
            .unwrap()
            .map(|f| f.unwrap().path())
            .find(|p| {
                p.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with("setup-")
            })
            .expect("a setup log");
        let out = std::fs::read_to_string(log).unwrap();
        assert!(out.contains("hello"));
        assert!(out.contains("PROJ-1"), "ran in the worktree: {out}");
        assert!(
            e.app
                .store
                .open_sessions(Some("PROJ-1"))
                .await
                .unwrap()
                .iter()
                .all(|s| s.kind != SessionKind::Setup)
        );
    }

    const FAILING: &str = "i=1; while [ $i -le 25 ]; do echo l$i; i=$((i+1)); done; exit 3";

    #[tokio::test]
    async fn exit_three_fails_with_the_last_twenty_lines_and_no_agent() {
        let e = env(FAILING).await;
        start(&e.app, &e.key, FAILING, launch()).await.unwrap();
        let reason =
            failed_reason(wait_for(&e, |s| matches!(s, TaskStatus::SetupFailed { .. })).await);
        let lines: Vec<&str> = reason.lines().collect();
        assert_eq!(lines.len(), 20, "{reason}");
        assert_eq!(lines[0], "l6");
        assert_eq!(lines[19], "l25");
        assert_eq!(agent_sessions(&e).await, 0);
        assert!(e.app.config.task_dir("PROJ-1").join(LAUNCH_FILE).exists());
    }

    #[tokio::test]
    async fn retry_runs_it_again_in_the_worktree() {
        let script = "test -f ran || { touch ran; exit 3; }";
        let e = env(script).await;
        start(&e.app, &e.key, script, launch()).await.unwrap();
        wait_for(&e, |s| matches!(s, TaskStatus::SetupFailed { .. })).await;
        assert!(e.worktree.join("ran").exists());
        retry(&e.app, &e.key).await.unwrap();
        wait_for(&e, |s| *s == TaskStatus::Running).await;
        assert_eq!(agent_sessions(&e).await, 1);
    }

    #[tokio::test]
    async fn skip_launches_the_agent_without_setup() {
        let e = env("exit 3").await;
        start(&e.app, &e.key, "exit 3", launch()).await.unwrap();
        wait_for(&e, |s| matches!(s, TaskStatus::SetupFailed { .. })).await;
        skip(&e.app, &e.key).await.unwrap();
        assert_eq!(status(&e).await, TaskStatus::Running);
        assert_eq!(agent_sessions(&e).await, 1);
        assert!(!e.app.config.task_dir("PROJ-1").join(LAUNCH_FILE).exists());
    }

    #[tokio::test]
    async fn cancel_fails_the_setup_as_cancelled() {
        let e = env("sleep 30").await;
        start(&e.app, &e.key, "sleep 30", launch()).await.unwrap();
        assert!(
            e.app
                .sessions
                .of_task("PROJ-1")
                .iter()
                .any(|l| l.kind == SessionKind::Setup)
        );
        cancel(&e.app, &e.key).await.unwrap();
        let reason =
            failed_reason(wait_for(&e, |s| matches!(s, TaskStatus::SetupFailed { .. })).await);
        assert_eq!(reason, "cancelled");
        assert_eq!(agent_sessions(&e).await, 0);
        assert!(cancel(&e.app, &e.key).await.is_err());
    }

    #[tokio::test]
    async fn cancel_stops_what_the_setup_started() {
        let script = "sleep 30 & echo $! > grandchild.pid; wait";
        let e = env(script).await;
        start(&e.app, &e.key, script, launch()).await.unwrap();
        let pid_file = e.worktree.join("grandchild.pid");
        for _ in 0..100 {
            if std::fs::read_to_string(&pid_file).is_ok_and(|s| !s.trim().is_empty()) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        let pid = std::fs::read_to_string(&pid_file)
            .unwrap()
            .trim()
            .to_string();
        let alive = |pid: &str| {
            std::process::Command::new("kill")
                .args(["-0", pid])
                .stderr(std::process::Stdio::null())
                .status()
                .unwrap()
                .success()
        };
        assert!(alive(&pid), "the grandchild runs before the cancel");
        cancel(&e.app, &e.key).await.unwrap();
        wait_for(&e, |s| matches!(s, TaskStatus::SetupFailed { .. })).await;
        for _ in 0..40 {
            if !alive(&pid) {
                return;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        panic!("the grandchild {pid} survived the cancel");
    }

    #[tokio::test]
    async fn retry_without_a_saved_launch_is_refused_and_leaves_the_task_alone() {
        let e = env("exit 3").await;
        start(&e.app, &e.key, "exit 3", launch()).await.unwrap();
        wait_for(&e, |s| matches!(s, TaskStatus::SetupFailed { .. })).await;
        std::fs::remove_file(e.app.config.task_dir("PROJ-1").join(LAUNCH_FILE)).unwrap();
        assert!(matches!(
            retry(&e.app, &e.key).await,
            Err(LaunchError::Unsupported(_))
        ));
        assert!(matches!(
            skip(&e.app, &e.key).await,
            Err(LaunchError::Unsupported(_))
        ));
        assert!(matches!(status(&e).await, TaskStatus::SetupFailed { .. }));
    }

    #[tokio::test]
    async fn a_setup_over_the_limit_is_stopped_and_fails() {
        let e = env("echo started; sleep 30").await;
        spawn_setup(
            &e.app,
            &e.key,
            "echo started; sleep 30",
            Duration::from_secs(1),
        )
        .await
        .unwrap();
        let reason =
            failed_reason(wait_for(&e, |s| matches!(s, TaskStatus::SetupFailed { .. })).await);
        assert!(reason.contains("(1 s)"), "{reason}");
        assert!(reason.contains("started"), "{reason}");
    }

    #[tokio::test]
    async fn a_task_left_preparing_with_no_setup_is_interrupted() {
        let e = env("true").await;
        recover_interrupted(&e.app).await.unwrap();
        assert_eq!(failed_reason(status(&e).await), "interrupted");
    }

    #[tokio::test]
    async fn recovery_leaves_a_live_setup_alone() {
        let e = env("sleep 30").await;
        start(&e.app, &e.key, "sleep 30", launch()).await.unwrap();
        recover_interrupted(&e.app).await.unwrap();
        assert_eq!(status(&e).await, TaskStatus::PreparingWorkspace);
        cancel(&e.app, &e.key).await.unwrap();
    }

    #[tokio::test]
    async fn the_setup_never_runs_in_the_projects_checkout() {
        let e = env("touch oops").await;
        let (parent, mut ws) = e.app.store.workspace(&e.key).await.unwrap().unwrap();
        ws.path = parent.clone();
        e.app
            .store
            .save_workspace(&e.key, &parent, &ws)
            .await
            .unwrap();
        assert!(start(&e.app, &e.key, "touch oops", launch()).await.is_err());
        assert!(!parent.join("oops").exists());
        assert_eq!(status(&e).await, TaskStatus::PreparingWorkspace);
    }

    #[test]
    fn tail_drops_escapes_and_keeps_the_end() {
        let dir = std::env::temp_dir().join(format!("delilah-tail-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let log = dir.join("l.log");
        std::fs::write(
            &log,
            "\x1b[31mred\x1b[0m\r\nprogress 1\rprogress 2\r\nend\r\n",
        )
        .unwrap();
        assert_eq!(output_tail(&log), "red\nprogress 2\nend");
    }
}
