//! Opening a PR from Delilah: the proposal the dialog starts from, then pushing the
//! branch, creating the PR with `gh` and linking it.

use std::path::{Path as FsPath, PathBuf};

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use delilah_core::TaskKey;
use delilah_git::GitOptions;
use delilah_integrations::jira::JiraMode;

use super::dto::{OpenRequest, PrView, Proposal, ProposalQuery};
use super::known_task;
use crate::api::ApiError;
use crate::gh::parse_pr_url;
use crate::jira::{JiraSettings, SETTINGS_KEY};
use crate::pr_loop;
use crate::pr_types::PrState;
use crate::state::AppState;
use crate::store_prs::PrRow;

fn is_open(row: &PrRow) -> bool {
    row.snapshot
        .as_ref()
        .is_none_or(|s| s.state == PrState::Open)
}

/// Where a PR of `repo` (default `.`) would come from: the task's branch and the folder.
async fn source_of(
    app: &AppState,
    key: &TaskKey,
    repo: Option<&str>,
) -> Result<(String, String, PathBuf, Vec<String>), ApiError> {
    let (_, ws) = app.store.workspace(key).await?.ok_or_else(|| {
        ApiError::conflict_reason(
            format!("{key} has no workspace to open a PR from"),
            "no-workspace",
        )
    })?;
    let repo = repo.unwrap_or(".");
    let repos = ws.repos();
    let dir = repos
        .iter()
        .find(|(label, _)| label == repo)
        .map(|(_, dir)| dir.clone())
        .ok_or_else(|| ApiError::bad_request(format!("{key} has no repo {repo}")))?;
    let labels = repos.into_iter().map(|(label, _)| label).collect();
    Ok((repo.to_string(), ws.branch, dir, labels))
}

fn git_options(app: &AppState) -> GitOptions {
    GitOptions {
        allow_file_protocol: app.config.allow_file_protocol,
    }
}

fn join_error(e: tokio::task::JoinError) -> ApiError {
    ApiError::internal(format!("git task failed: {e}"))
}

/// The Jira link of the ticket, when Jira is connected for real and the task is one.
async fn jira_link(app: &AppState, key: &TaskKey) -> Option<String> {
    if key.as_str().starts_with("LIBRE-") {
        return None;
    }
    let settings: JiraSettings = app.store.setting(SETTINGS_KEY).await.ok().flatten()?;
    if settings.mode == JiraMode::Sample {
        return None;
    }
    let site = settings.site?;
    Some(format!(
        "{}/browse/{key}",
        site.trim().trim_end_matches('/')
    ))
}

/// The default branch of the repo at `dir`.
async fn base_of(app: &AppState, dir: &FsPath) -> Result<String, ApiError> {
    Ok(app
        .gh
        .repo_view(dir)
        .await
        .map_err(|e| ApiError::conflict(e.to_string()))?
        .default_branch)
}

/// What the branch holds beyond the default branch of its repo.
struct Commits {
    base: String,
    subjects: Vec<String>,
    needs_push: bool,
}

async fn commits_of(app: &AppState, dir: &FsPath, branch: &str) -> Result<Commits, ApiError> {
    let base = base_of(app, dir).await?;
    let opts = git_options(app);
    let (d, b, bs) = (dir.to_path_buf(), branch.to_string(), base.clone());
    let (subjects, needs_push) = tokio::task::spawn_blocking(move || {
        let subjects = delilah_git::commit_subjects(&d, &bs, &b, &opts)?;
        let needs_push = delilah_git::needs_push(&d, &b, &opts)?;
        Ok::<_, delilah_git::GitError>((subjects, needs_push))
    })
    .await
    .map_err(join_error)?
    .map_err(|e| ApiError::conflict(e.to_string()))?;
    Ok(Commits {
        base,
        subjects,
        needs_push,
    })
}

/// `GET /tasks/{key}/prs/proposal?repo=.`: the title and body the dialog starts from.
pub async fn proposal(
    State(app): State<AppState>,
    Path(key): Path<String>,
    Query(q): Query<ProposalQuery>,
) -> Result<Json<Proposal>, ApiError> {
    let key = known_task(&app, &key).await?;
    let rows = app.store.prs_of(&key).await?;
    let has_open = |label: &str| rows.iter().any(|r| r.repo == label && is_open(r));
    let mut candidates = vec![q.repo.clone().unwrap_or_else(|| ".".into())];
    if candidates[0] == "."
        && let Some((_, ws)) = app.store.workspace(&key).await?
    {
        // A parent has no "." repo: the default is its first submodule without a PR
        // that has commits to open one with.
        let labels: Vec<String> = ws.repos().into_iter().map(|(l, _)| l).collect();
        if !labels.iter().any(|l| l == ".") {
            candidates = labels.into_iter().filter(|l| !has_open(l)).collect();
            if candidates.is_empty() {
                return Err(ApiError::conflict_reason(
                    "every repo already has an open PR",
                    "has-pr",
                ));
            }
        }
    }
    let mut empty = None;
    let mut picked = None;
    for candidate in &candidates {
        let (repo, branch, dir, labels) = source_of(&app, &key, Some(candidate)).await?;
        if has_open(&repo) {
            return Err(ApiError::conflict_reason(
                format!("{repo} already has an open PR"),
                "has-pr",
            ));
        }
        let commits = commits_of(&app, &dir, &branch).await?;
        if commits.subjects.is_empty() {
            empty.get_or_insert((branch, commits.base));
        } else {
            picked = Some((repo, labels, commits));
            break;
        }
    }
    let Some((repo, labels, commits)) = picked else {
        let (branch, base) = empty.unwrap_or_default();
        return Err(ApiError::conflict_reason(
            format!("{branch} has no commits beyond {base}"),
            "no-commits",
        ));
    };
    let mut body = String::new();
    if let Some(link) = jira_link(&app, &key).await {
        body.push_str(&format!("Jira: {link}\n\n"));
    }
    for subject in &commits.subjects {
        body.push_str(&format!("- {subject}\n"));
    }
    let title = app
        .store
        .get_task(&key)
        .await?
        .map(|t| t.title)
        .unwrap_or_default();
    Ok(Json(Proposal {
        repo,
        title,
        body: body.trim_end().to_string(),
        base: commits.base,
        needs_push: commits.needs_push,
        repos: labels.into_iter().filter(|l| !has_open(l)).collect(),
    }))
}

/// Pushes `branch` from `dir` unless the remote has it already.
async fn push_if_needed(app: &AppState, dir: &FsPath, branch: &str) -> Result<(), ApiError> {
    let opts = git_options(app);
    let (d, b) = (dir.to_path_buf(), branch.to_string());
    tokio::task::spawn_blocking(move || {
        if delilah_git::needs_push(&d, &b, &opts)? {
            delilah_git::push_branch(&d, &b, &opts)?;
        }
        Ok::<_, delilah_git::GitError>(())
    })
    .await
    .map_err(join_error)?
    .map_err(|e| ApiError::conflict(e.to_string()))
}

/// Creates the PR of `branch` in `repo` (at `dir`) and links it to that very repo.
async fn create_and_link(
    app: &AppState,
    key: &TaskKey,
    (repo, dir, branch): (String, &FsPath, &str),
    req: &OpenRequest,
    title: &str,
) -> Result<PrRow, ApiError> {
    let base = base_of(app, dir).await?;
    let url = app
        .gh
        .pr_create(dir, branch, &base, title, &req.body, req.draft)
        .await
        .map_err(|e| ApiError::conflict(e.to_string()))?;
    let pr = parse_pr_url(&url)
        .ok_or_else(|| ApiError::conflict(format!("gh answered with no PR URL: {url}")))?;
    let (_, snapshot) = app
        .gh
        .pr_view_url(&pr.url())
        .await
        .map_err(|e| ApiError::conflict(e.to_string()))?;
    Ok(pr_loop::link_known(app, key, repo, pr, &snapshot, "opened").await?)
}

/// `POST /tasks/{key}/prs`: pushes the branch if needed, creates the PR and links it.
pub async fn open(
    State(app): State<AppState>,
    Path(key): Path<String>,
    Json(req): Json<OpenRequest>,
) -> Result<(StatusCode, Json<PrView>), ApiError> {
    let key = known_task(&app, &key).await?;
    let title = req.title.trim();
    if title.is_empty() {
        return Err(ApiError::bad_request("the PR needs a title"));
    }
    let (repo, branch, dir, _) = source_of(&app, &key, req.repo.as_deref()).await?;
    // Keeps a detection from linking the PR twice while it is being opened; the link
    // is an upsert anyway, so a check already running is not waited for.
    let _claim = pr_loop::claim(&app, &key);
    if app
        .store
        .prs_of(&key)
        .await?
        .iter()
        .any(|r| r.repo == repo && is_open(r))
    {
        return Err(ApiError::conflict_reason(
            format!("{repo} already has an open PR"),
            "has-pr",
        ));
    }
    push_if_needed(&app, &dir, &branch).await?;
    let row = create_and_link(&app, &key, (repo, &dir, &branch), &req, title).await?;
    Ok((StatusCode::CREATED, Json(PrView::from(&row))))
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::sync::Arc;

    use axum::Router;
    use serde_json::json;

    use super::*;
    use crate::gh::GhError;
    use crate::gh::fake::{FakeGh, PR_URL};
    use crate::pr_api::support::{call, fake, setup};
    use crate::pr_loop;
    use crate::pr_testkit::TempDir;

    fn opening_gh() -> Arc<FakeGh> {
        Arc::new(
            FakeGh::happy()
                .with_user("me")
                .on(&["pr", "create"], Ok(PR_URL)),
        )
    }

    async fn git_setup(gh: Arc<FakeGh>, subjects: &[&str]) -> (Router, AppState, TempDir) {
        let (router, app, dir) = setup(gh).await;
        crate::pr_testkit::git_repo_with_origin(
            &dir.path().join("ws"),
            &dir.path().join("origin"),
            "feature/PROJ-1",
            subjects,
        );
        (router, app, dir)
    }

    fn jira(site: &str) -> JiraSettings {
        JiraSettings {
            mode: JiraMode::Cloud,
            site: Some(site.into()),
            email: None,
            jql: String::new(),
        }
    }

    #[tokio::test]
    async fn the_proposal_lists_commits_and_links_the_jira_ticket() {
        let (router, app, _dir) = git_setup(fake(), &["first", "second"]).await;
        app.store
            .set_setting(SETTINGS_KEY, &jira("https://acme.atlassian.net/"))
            .await
            .unwrap();
        let (s, v) = call(&router, "GET", "/api/v1/tasks/PROJ-1/prs/proposal", None).await;
        assert_eq!(s, StatusCode::OK);
        assert_eq!(v["title"], "t");
        assert_eq!(v["base"], "main");
        assert_eq!(v["needs_push"], true);
        assert_eq!(v["repo"], ".");
        assert_eq!(v["repos"], json!(["."]));
        assert_eq!(
            v["body"],
            "Jira: https://acme.atlassian.net/browse/PROJ-1\n\n- first\n- second"
        );
    }

    #[tokio::test]
    async fn a_free_task_or_no_jira_has_no_link() {
        let (router, app, _dir) = git_setup(fake(), &["first"]).await;
        let (_, v) = call(&router, "GET", "/api/v1/tasks/PROJ-1/prs/proposal", None).await;
        assert_eq!(v["body"], "- first");
        // A LIBRE- key never links, even with Jira connected.
        app.store
            .set_setting(SETTINGS_KEY, &jira("https://acme.atlassian.net"))
            .await
            .unwrap();
        let mut task = delilah_core::Task::new(TaskKey::new("LIBRE-1"), "free", "feature");
        task.project = Some("proj".into());
        app.store.create_task(&task).await.unwrap();
        let ws = app
            .store
            .workspace(&TaskKey::new("PROJ-1"))
            .await
            .unwrap()
            .unwrap();
        app.store
            .save_workspace(&TaskKey::new("LIBRE-1"), &ws.0, &ws.1)
            .await
            .unwrap();
        let (s, v) = call(&router, "GET", "/api/v1/tasks/LIBRE-1/prs/proposal", None).await;
        assert_eq!(s, StatusCode::OK);
        assert_eq!(v["body"], "- first");
    }

    #[tokio::test]
    async fn the_proposal_is_409_without_commits_or_with_an_open_pr() {
        let (router, app, _dir) = git_setup(fake(), &[]).await;
        let (s, v) = call(&router, "GET", "/api/v1/tasks/PROJ-1/prs/proposal", None).await;
        assert_eq!(s, StatusCode::CONFLICT);
        assert_eq!(v["reason"], "no-commits");

        pr_loop::check_task(&app, &TaskKey::new("PROJ-1"), true).await;
        let (s, v) = call(&router, "GET", "/api/v1/tasks/PROJ-1/prs/proposal", None).await;
        assert_eq!(s, StatusCode::CONFLICT);
        assert_eq!(v["reason"], "has-pr");
        let (s, _) = call(
            &router,
            "GET",
            "/api/v1/tasks/PROJ-1/prs/proposal?repo=nope",
            None,
        )
        .await;
        assert_eq!(s, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn opening_pushes_creates_links_and_moves_to_review() {
        let gh = opening_gh();
        let (router, app, dir) = git_setup(gh.clone(), &["first"]).await;
        let body = json!({ "repo": ".", "title": "My PR", "body": "the body", "draft": true });
        let (s, v) = call(&router, "POST", "/api/v1/tasks/PROJ-1/prs", Some(body)).await;
        assert_eq!(s, StatusCode::CREATED, "{v}");
        assert_eq!(v["linked_by"], "opened");
        assert_eq!(v["url"], PR_URL);

        let calls = gh.calls();
        let create = calls
            .iter()
            .position(|c| c.starts_with(&["pr".into(), "create".into()]))
            .unwrap();
        let args = &calls[create];
        for needle in [
            "--head",
            "feature/PROJ-1",
            "--base",
            "main",
            "--title",
            "My PR",
            "--body-file",
            "-",
            "--draft",
        ] {
            assert!(args.iter().any(|a| a == needle), "{needle} in {args:?}");
        }
        assert_eq!(gh.stdins()[create].as_deref(), Some("the body"));
        let pushed = std::process::Command::new("git")
            .args(["rev-parse", "--verify", "feature/PROJ-1"])
            .current_dir(dir.path().join("origin"))
            .output()
            .unwrap();
        assert!(pushed.status.success());
        let task = app
            .store
            .get_task(&TaskKey::new("PROJ-1"))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(task.phase.as_deref(), Some("review"));

        let body = json!({ "title": "again" });
        let (s, _) = call(&router, "POST", "/api/v1/tasks/PROJ-1/prs", Some(body)).await;
        assert_eq!(s, StatusCode::CONFLICT);
        let body = json!({ "title": "  " });
        let (s, _) = call(&router, "POST", "/api/v1/tasks/PROJ-1/prs", Some(body)).await;
        assert_eq!(s, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn a_failed_push_is_409_with_gits_stderr_and_links_nothing() {
        let gh = opening_gh();
        let (router, app, dir) = git_setup(gh.clone(), &["first"]).await;
        std::fs::remove_dir_all(dir.path().join("origin")).unwrap();
        let body = json!({ "title": "T" });
        let (s, v) = call(&router, "POST", "/api/v1/tasks/PROJ-1/prs", Some(body)).await;
        assert_eq!(s, StatusCode::CONFLICT);
        assert!(v["error"].as_str().unwrap().contains("git"));
        assert!(
            app.store
                .prs_of(&TaskKey::new("PROJ-1"))
                .await
                .unwrap()
                .is_empty()
        );
        assert!(gh.calls().iter().all(|c| c[..2] != ["pr", "create"]));
    }

    #[tokio::test]
    async fn a_gh_failure_on_create_is_409_and_links_nothing() {
        let gh = Arc::new(FakeGh::happy().on(
            &["pr", "create"],
            Err(GhError::Failed {
                code: Some(1),
                stderr: "no way".into(),
            }),
        ));
        let (router, app, _dir) = git_setup(gh, &["first"]).await;
        let (s, v) = call(
            &router,
            "POST",
            "/api/v1/tasks/PROJ-1/prs",
            Some(json!({ "title": "T" })),
        )
        .await;
        assert_eq!(s, StatusCode::CONFLICT);
        assert!(v["error"].as_str().unwrap().contains("no way"));
        assert!(
            app.store
                .prs_of(&TaskKey::new("PROJ-1"))
                .await
                .unwrap()
                .is_empty()
        );
    }

    /// A parent task with submodules `svc-a` and `svc-b`, each a repo with its origin.
    async fn parent_setup(gh: Arc<FakeGh>) -> (Router, AppState, TempDir) {
        parent_setup_with(gh, &["svc-a"]).await
    }

    /// Like `parent_setup`, with `a_subjects` as the commits of `svc-a`.
    async fn parent_setup_with(
        gh: Arc<FakeGh>,
        a_subjects: &[&str],
    ) -> (Router, AppState, TempDir) {
        let (router, app, dir) = setup(gh).await;
        let ws_path = dir.path().join("ws");
        for sub in ["svc-a", "svc-b"] {
            let own = [sub];
            let subjects: &[&str] = if sub == "svc-a" { a_subjects } else { &own };
            crate::pr_testkit::git_repo_with_origin(
                &ws_path.join(sub),
                &dir.path().join(format!("origin-{sub}")),
                "feature/PROJ-1",
                subjects,
            );
        }
        let ws = delilah_git::Workspace {
            path: ws_path,
            branch: "feature/PROJ-1".into(),
            kind: delilah_core::ProjectKind::Parent,
            submodules: vec!["svc-a".into(), "svc-b".into()],
        };
        app.store
            .save_workspace(&TaskKey::new("PROJ-1"), Path::new("/parent"), &ws)
            .await
            .unwrap();
        (router, app, dir)
    }

    #[tokio::test]
    async fn a_parent_defaults_to_its_first_free_submodule_and_opens_per_submodule() {
        let gh = opening_gh();
        let (router, app, dir) = parent_setup(gh).await;
        let (s, v) = call(&router, "GET", "/api/v1/tasks/PROJ-1/prs/proposal", None).await;
        assert_eq!(s, StatusCode::OK, "{v}");
        assert_eq!(v["repo"], "svc-a");
        assert_eq!(v["repos"], json!(["svc-a", "svc-b"]));

        let (s, _) = call(
            &router,
            "GET",
            "/api/v1/tasks/PROJ-1/prs/proposal?repo=nope",
            None,
        )
        .await;
        assert_eq!(s, StatusCode::BAD_REQUEST);
        let body = json!({ "repo": ".", "title": "T" });
        let (s, _) = call(&router, "POST", "/api/v1/tasks/PROJ-1/prs", Some(body)).await;
        assert_eq!(s, StatusCode::BAD_REQUEST);

        let body = json!({ "repo": "svc-b", "title": "T" });
        let (s, v) = call(&router, "POST", "/api/v1/tasks/PROJ-1/prs", Some(body)).await;
        assert_eq!(s, StatusCode::CREATED, "{v}");
        // The fake gh answers `repo view` alike for every folder, yet the row is svc-b's.
        let rows = app.store.prs_of(&TaskKey::new("PROJ-1")).await.unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].repo, "svc-b");
        assert_eq!(rows[0].linked_by, "opened");
        let origin = dir.path().join("origin-svc-b");
        let pushed = std::process::Command::new("git")
            .args(["rev-parse", "--verify", "feature/PROJ-1"])
            .current_dir(&origin)
            .output()
            .unwrap();
        assert!(pushed.status.success());
        let untouched = std::process::Command::new("git")
            .args(["rev-parse", "--verify", "feature/PROJ-1"])
            .current_dir(dir.path().join("origin-svc-a"))
            .output()
            .unwrap();
        assert!(!untouched.status.success());
    }

    #[tokio::test]
    async fn a_parent_defaults_to_the_first_free_submodule_with_commits() {
        let (router, _app, _dir) = parent_setup_with(opening_gh(), &[]).await;
        let (s, v) = call(&router, "GET", "/api/v1/tasks/PROJ-1/prs/proposal", None).await;
        assert_eq!(s, StatusCode::OK, "{v}");
        assert_eq!(v["repo"], "svc-b");
        assert_eq!(v["repos"], json!(["svc-a", "svc-b"]));
        assert_eq!(v["body"], "- svc-b");
    }

    #[tokio::test]
    async fn a_parent_with_no_commits_in_any_free_submodule_is_409_no_commits() {
        let (router, _app, dir) = parent_setup_with(opening_gh(), &[]).await;
        // svc-b has none either: reset it to its base.
        let b = dir.path().join("ws/svc-b");
        let out = std::process::Command::new("git")
            .args(["reset", "--hard", "main"])
            .current_dir(&b)
            .output()
            .unwrap();
        assert!(out.status.success());
        let (s, v) = call(&router, "GET", "/api/v1/tasks/PROJ-1/prs/proposal", None).await;
        assert_eq!(s, StatusCode::CONFLICT);
        assert_eq!(v["reason"], "no-commits");
    }
}
