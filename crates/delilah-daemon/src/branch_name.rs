//! Names the branch of a task that is starting: asks Claude for a short name (one-shot,
//! no session, no workspace), falls back to the title and then to the key, and makes the
//! result unique across the repos the task will touch.

use std::path::PathBuf;
use std::time::Duration;

use delilah_agents::{branch_name, oneshot};
use delilah_core::{Project, ProjectKind, branch};
use delilah_git::GitOptions;
use serde::Serialize;

use crate::api::ApiError;
use crate::state::AppState;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    Claude,
    Title,
    Key,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Proposal {
    /// `None` when there is neither a usable name nor a key (a free task whose title
    /// has nothing usable and whose Claude call failed).
    pub branch: Option<String>,
    pub source: Source,
}

/// The repos whose branches must not clash with the new one.
fn dirs_of(project: &Project, submodules: &[String]) -> Vec<PathBuf> {
    match project.kind {
        ProjectKind::Repo => vec![project.path.clone()],
        ProjectKind::Parent => submodules.iter().map(|s| project.path.join(s)).collect(),
    }
}

/// `base`, or `base-2`.. if some repo of the task already has a branch of that name.
/// A repo that cannot be read leaves `base` as it is: creating the workspace reports it.
pub async fn unique(
    app: &AppState,
    project: &Project,
    submodules: &[String],
    base: String,
) -> Result<String, ApiError> {
    let dirs = dirs_of(project, submodules);
    let opts = GitOptions {
        allow_file_protocol: app.config.allow_file_protocol,
    };
    let wanted = base.clone();
    let free = tokio::task::spawn_blocking(move || delilah_git::free_branch(&dirs, &wanted, &opts))
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?;
    Ok(free.unwrap_or_else(|e| {
        tracing::warn!(branch = %base, "cannot check whether the branch is taken: {e}");
        base
    }))
}

/// Asks Claude for a short branch name; `None` on any failure or when it takes too long.
async fn ask_claude(
    app: &AppState,
    title: &str,
    notes: Option<&str>,
    limit: Duration,
) -> Option<String> {
    let prompt = branch_name::prompt(title, notes);
    let mut cmd = tokio::process::Command::new(&app.config.claude_bin);
    cmd.env_clear()
        .envs(oneshot::env())
        .args(branch_name::args(&prompt))
        .current_dir(std::env::temp_dir())
        .stdin(std::process::Stdio::null())
        .kill_on_drop(true);
    let output = match tokio::time::timeout(limit, cmd.output()).await {
        Ok(Ok(output)) => output,
        Ok(Err(e)) => {
            tracing::warn!("cannot run claude to name the branch: {e}");
            return None;
        }
        Err(_) => {
            tracing::warn!("claude took more than {limit:?} to name the branch");
            return None;
        }
    };
    match branch_name::parse(&String::from_utf8_lossy(&output.stdout)) {
        Ok(name) => Some(name),
        Err(e) => {
            tracing::warn!("claude gave no usable branch name: {e}");
            None
        }
    }
}

/// The branch a starting task gets: Claude's name, else the title's, else the key's,
/// made unique. Never fails because of Claude.
#[allow(clippy::too_many_arguments)]
pub async fn propose(
    app: &AppState,
    project: &Project,
    key: Option<&str>,
    title: &str,
    notes: Option<&str>,
    free: bool,
    submodules: &[String],
    limit: Duration,
) -> Result<Proposal, ApiError> {
    let from_claude = ask_claude(app, title, notes, limit)
        .await
        .filter(|name| !branch::slug(name, 40).is_empty())
        .and_then(|name| branch::compose(key, &name, free));
    let title_short = branch::title_slug(title);
    let (base, source) = match from_claude {
        Some(name) => (Some(name), Source::Claude),
        None => match (title_short.is_empty(), key) {
            (false, _) => (branch::compose(key, &title_short, free), Source::Title),
            (true, Some(key)) => (branch::fallback(key, free), Source::Key),
            (true, None) => (None, Source::Key),
        },
    };
    let branch = match base {
        Some(base) => Some(unique(app, project, submodules, base).await?),
        None => None,
    };
    tracing::info!(source = ?source, branch = ?branch, "named the branch");
    Ok(Proposal { branch, source })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    use std::path::Path;

    fn git(dir: &Path, args: &[&str]) {
        let out = std::process::Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(["-c", "user.name=t", "-c", "user.email=t@t"])
            .args(args)
            .output()
            .unwrap();
        assert!(out.status.success(), "git {args:?}");
    }

    /// An app whose `claude` is the given shell script body, and a repo project.
    async fn setup(script: &str) -> (AppState, Project) {
        let home = std::env::temp_dir().join(format!("delilah-bn-{}", uuid::Uuid::new_v4()));
        let repo = home.join("web");
        std::fs::create_dir_all(&repo).unwrap();
        git(&repo, &["init", "-q", "-b", "main"]);
        git(&repo, &["commit", "-q", "--allow-empty", "-m", "init"]);
        let bin = home.join("claude");
        std::fs::write(&bin, format!("#!/bin/sh\n{script}\n")).unwrap();
        std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
        let config = crate::config::Config {
            home,
            addr: crate::config::DEFAULT_ADDR.parse().unwrap(),
            allow_file_protocol: false,
            notify: false,
            ui_origin: None,
            claude_bin: bin,
        };
        let app = AppState::new(crate::store::Store::in_memory().await.unwrap(), config);
        let project = Project {
            name: "web".into(),
            path: repo,
            kind: ProjectKind::Repo,
            workflow: "feature".into(),
        };
        (app, project)
    }

    const SAYS_NAME: &str =
        r#"echo '{"is_error":false,"result":"{\"name\":\"retry failed payments\"}"}'"#;
    const LIMIT: Duration = Duration::from_secs(5);

    async fn ask(
        script: &str,
        key: Option<&str>,
        title: &str,
        free: bool,
    ) -> (Proposal, AppState, Project) {
        let (app, project) = setup(script).await;
        let p = propose(&app, &project, key, title, None, free, &[], LIMIT)
            .await
            .ok()
            .unwrap();
        (p, app, project)
    }

    #[tokio::test]
    async fn claude_names_a_jira_branch() {
        let (p, ..) = ask(SAYS_NAME, Some("PROJ-12"), "Reintentar pagos", false).await;
        assert_eq!(
            p.branch.as_deref(),
            Some("feature/PROJ-12-retry-failed-payments")
        );
        assert_eq!(p.source, Source::Claude);
        assert_eq!(serde_json::to_value(&p).unwrap()["source"], "claude");
    }

    #[tokio::test]
    async fn a_free_task_branch_has_no_key() {
        let (p, ..) = ask(SAYS_NAME, Some("LIBRE-3"), "x", true).await;
        assert_eq!(p.branch.as_deref(), Some("feature/retry-failed-payments"));
    }

    #[tokio::test]
    async fn a_failing_claude_gives_the_title_slug() {
        for script in [
            "exit 1",
            r#"echo '{"is_error":true,"result":"boom"}'"#,
            "echo not json",
        ] {
            let (p, ..) = ask(script, Some("PROJ-12"), "Retry failed payments", false).await;
            assert_eq!(
                p.branch.as_deref(),
                Some("feature/PROJ-12-retry-failed-payments"),
                "{script}"
            );
            assert_eq!(p.source, Source::Title);
        }
    }

    #[tokio::test]
    async fn a_slow_claude_gives_the_title_slug() {
        let (app, project) = setup("sleep 5").await;
        let p = propose(
            &app,
            &project,
            Some("PROJ-12"),
            "Retry payments",
            None,
            false,
            &[],
            Duration::from_millis(200),
        )
        .await
        .ok()
        .unwrap();
        assert_eq!(p.branch.as_deref(), Some("feature/PROJ-12-retry-payments"));
        assert_eq!(p.source, Source::Title);
    }

    #[tokio::test]
    async fn an_unusable_title_falls_back_to_the_key_or_nothing() {
        let (p, ..) = ask("exit 1", Some("LIBRE-4"), "!!", true).await;
        assert_eq!(p.branch.as_deref(), Some("feature/libre-4"));
        assert_eq!(p.source, Source::Key);
        let (p, ..) = ask("exit 1", Some("PROJ-4"), "!!", false).await;
        assert_eq!(p.branch.as_deref(), Some("feature/PROJ-4"));
        let (p, ..) = ask("exit 1", None, "!!", true).await;
        assert_eq!(p.branch, None);
        assert_eq!(p.source, Source::Key);
    }

    #[tokio::test]
    async fn a_taken_name_gets_a_suffix() {
        let (app, project) = setup(SAYS_NAME).await;
        git(&project.path, &["branch", "feature/retry-failed-payments"]);
        let p = propose(&app, &project, None, "x", None, true, &[], LIMIT)
            .await
            .ok()
            .unwrap();
        assert_eq!(p.branch.as_deref(), Some("feature/retry-failed-payments-2"));
    }
}
