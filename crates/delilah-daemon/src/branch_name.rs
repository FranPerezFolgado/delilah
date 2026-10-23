//! Names the branch of a task that is starting: asks Claude for a short name (one-shot,
//! no session, no workspace), falls back to the title and then to the key, and makes the
//! result unique across the repos the task will touch.

use std::path::{Component, Path as FsPath, PathBuf};
use std::time::Duration;

use axum::Json;
use axum::extract::{Path, State};
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
        // An entry that leaves the project (`..`, absolute) would make git read elsewhere.
        ProjectKind::Parent => submodules
            .iter()
            .filter(|s| {
                FsPath::new(s)
                    .components()
                    .all(|c| matches!(c, Component::Normal(_)))
            })
            .map(|s| project.path.join(s))
            .collect(),
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

/// Claude's text as a branch: its own `feature/` and the ticket key are not repeated.
/// `None` when nothing but the key is left.
fn claude_branch(name: &str, key: Option<&str>, free: bool) -> Option<String> {
    let named = branch::custom_for(name, key, free)?;
    let Some(keyed) = key
        .filter(|_| !free)
        .and_then(|k| branch::compose(Some(k), "", false))
    else {
        return Some(named);
    };
    if named == keyed {
        return None;
    }
    if named.starts_with(&format!("{keyed}-")) {
        return Some(named);
    }
    branch::compose(key, named.strip_prefix(branch::PREFIX)?, false)
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
        .and_then(|name| claude_branch(&name, key, free));
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

/// How long Claude gets to name the branch of a starting task.
const START_NAME_LIMIT: Duration = Duration::from_secs(5);

/// The branch a task being started gets: the one the user typed (or accepted from a
/// proposal), normalised; else a proposal, else the key's. Unique across its repos.
#[allow(clippy::too_many_arguments)]
pub async fn for_start(
    app: &AppState,
    project: &Project,
    key: &str,
    title: &str,
    notes: Option<&str>,
    free: bool,
    submodules: &[String],
    branch: Option<&str>,
) -> Result<String, ApiError> {
    let nothing = || ApiError::bad_request("the branch name has nothing usable");
    let base = match branch {
        Some(typed) => branch::custom_for(typed, Some(key), free).ok_or_else(nothing)?,
        None => {
            let proposal = propose(
                app,
                project,
                Some(key),
                title,
                notes,
                free,
                submodules,
                START_NAME_LIMIT,
            )
            .await?;
            return proposal.branch.ok_or_else(nothing);
        }
    };
    unique(app, project, submodules, base).await
}

/// How long Claude gets to name a branch the user is still typing the task for.
const PROPOSE_LIMIT: Duration = Duration::from_secs(10);

#[derive(Debug, serde::Deserialize)]
pub struct ProposeRequest {
    /// Left out for a new free task.
    #[serde(default)]
    pub key: Option<String>,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub notes: Option<String>,
    #[serde(default)]
    pub free: bool,
    #[serde(default)]
    pub submodules: Vec<String>,
}

/// `POST /api/v1/projects/{name}/branch-name`: the branch a task would get, without
/// creating anything.
pub async fn propose_branch(
    State(app): State<AppState>,
    Path(name): Path<String>,
    Json(req): Json<ProposeRequest>,
) -> Result<Json<Proposal>, ApiError> {
    let project = app.store.project(&name).await?;
    if req.title.trim().is_empty() {
        return Err(ApiError::bad_request("say what the task is about"));
    }
    let key = req.key.as_deref().map(str::trim).filter(|k| !k.is_empty());
    let proposal = propose(
        &app,
        &project,
        key,
        req.title.trim(),
        req.notes.as_deref(),
        req.free,
        &req.submodules,
        PROPOSE_LIMIT,
    )
    .await?;
    Ok(Json(proposal))
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
            agent_program: None,
            ..crate::config::Config::test_defaults()
        };
        let app = AppState::new(crate::store::Store::in_memory().await.unwrap(), config);
        let project = Project {
            name: "web".into(),
            path: repo,
            kind: ProjectKind::Repo,
            workflow: "feature".into(),
            preview_command: None,
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
    async fn claude_repeating_feature_or_the_key_is_not_doubled() {
        let says =
            |n: &str| format!(r#"echo '{{"is_error":false,"result":"{{\"name\":\"{n}\"}}"}}'"#);
        for name in [
            "feature/PROJ-12 retry payments",
            "proj-12 retry payments",
            "feature/retry payments",
        ] {
            let (p, ..) = ask(&says(name), Some("PROJ-12"), "x", false).await;
            assert_eq!(
                p.branch.as_deref(),
                Some("feature/PROJ-12-retry-payments"),
                "{name}"
            );
            assert_eq!(p.source, Source::Claude);
        }
        let (p, ..) = ask(&says("feature/retry payments"), None, "x", true).await;
        assert_eq!(p.branch.as_deref(), Some("feature/retry-payments"));
    }

    #[test]
    fn submodules_leaving_the_project_are_skipped() {
        let project = Project {
            name: "p".into(),
            path: PathBuf::from("/p"),
            kind: ProjectKind::Parent,
            workflow: "feature".into(),
            preview_command: None,
        };
        let subs: Vec<String> = ["a", "../../x", "/etc", "b/c", "a/../.."]
            .map(String::from)
            .into();
        assert_eq!(
            dirs_of(&project, &subs),
            vec![PathBuf::from("/p/a"), PathBuf::from("/p/b/c")]
        );
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

    async fn post(app: AppState, uri: &str, body: serde_json::Value) -> (u16, serde_json::Value) {
        use tower::ServiceExt;
        let req = axum::http::Request::post(uri)
            .header("content-type", "application/json")
            .body(axum::body::Body::from(body.to_string()))
            .unwrap();
        let res = crate::api::router(app).oneshot(req).await.unwrap();
        let status = res.status().as_u16();
        let bytes = http_body_util::BodyExt::collect(res.into_body())
            .await
            .unwrap()
            .to_bytes();
        (status, serde_json::from_slice(&bytes).unwrap_or_default())
    }

    #[tokio::test]
    async fn the_endpoint_proposes_a_branch() {
        let (app, project) = setup(SAYS_NAME).await;
        app.store.create_project(&project).await.unwrap();
        let body = serde_json::json!({"key": "PROJ-12", "title": "Reintentar pagos"});
        let (status, json) = post(app.clone(), "/api/v1/projects/web/branch-name", body).await;
        assert_eq!(status, 200, "{json}");
        assert_eq!(json["branch"], "feature/PROJ-12-retry-failed-payments");
        assert_eq!(json["source"], "claude");
    }

    #[tokio::test]
    async fn the_endpoint_falls_back_to_the_title() {
        let (app, project) = setup("exit 1").await;
        app.store.create_project(&project).await.unwrap();
        let body = serde_json::json!({"title": "Retry payments", "free": true});
        let (status, json) = post(app, "/api/v1/projects/web/branch-name", body).await;
        assert_eq!(status, 200, "{json}");
        assert_eq!(json["branch"], "feature/retry-payments");
        assert_eq!(json["source"], "title");
    }

    #[tokio::test]
    async fn the_endpoint_refuses_an_empty_title_and_an_unknown_project() {
        let (app, project) = setup(SAYS_NAME).await;
        app.store.create_project(&project).await.unwrap();
        let (status, _) = post(
            app.clone(),
            "/api/v1/projects/web/branch-name",
            serde_json::json!({"title": "  "}),
        )
        .await;
        assert_eq!(status, 400);
        let (status, _) = post(
            app,
            "/api/v1/projects/nope/branch-name",
            serde_json::json!({"title": "x"}),
        )
        .await;
        assert_eq!(status, 404);
    }
}
