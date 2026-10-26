//! Brings a task's branch up to date with its base: how far behind it is (`GET
//! /tasks/{key}/base-status`) and merging the base into it (`POST
//! /tasks/{key}/update-branch`). The base is the one a new branch starts from (decision
//! 66: `origin/HEAD`, `origin/main`, `origin/master`, `main`, `master`), fetched first.
//! The task's work is never rewritten: it is a merge, refused if the workspace has
//! tracked changes, and undone if it conflicts.

use std::path::{Path, PathBuf};
use std::process::Command;

use axum::Json;
use axum::extract::{Path as UrlPath, State};
use delilah_core::TaskKey;
use delilah_git::{Deadline, GitOptions, Workspace};
use serde::Serialize;

use crate::api::ApiError;
use crate::events::Topic;
use crate::state::AppState;
use crate::store_prs::KIND_BASE_STALE;

/// How long the fetch of a status or an update may take.
const FETCH_LIMIT: std::time::Duration = std::time::Duration::from_secs(20);

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct BaseStatus {
    /// The ref the branch is compared with; `None` when no base could be resolved.
    pub base: Option<String>,
    /// Commits of the base the branch does not have yet (all the task's repos together).
    pub behind_by: u32,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct BranchUpdate {
    pub base: Option<String>,
    /// Commits brought in; 0: it was up to date already.
    pub merged: u32,
}

struct RepoState {
    label: String,
    dir: PathBuf,
    /// The task's branch, which the repo must be on.
    branch: String,
    base: Option<String>,
    behind: u32,
}

/// Runs git in `dir` without ever prompting; (succeeded, stdout, stderr).
fn run(dir: &Path, args: &[&str]) -> Result<(bool, String, String), ApiError> {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_EDITOR", "true")
        .output()
        .map_err(|e| ApiError::internal(format!("could not run git: {e}")))?;
    Ok((
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).trim().to_string(),
        String::from_utf8_lossy(&out.stderr).trim().to_string(),
    ))
}

/// The first usable base ref the repo has locally (the order of `prepare_base`).
fn resolve_base(dir: &Path) -> Result<Option<String>, ApiError> {
    let head = run(
        dir,
        &["symbolic-ref", "--short", "refs/remotes/origin/HEAD"],
    )?;
    let from_origin = head.0.then_some(head.1).filter(|s| !s.is_empty());
    for r in from_origin.into_iter().chain(
        ["origin/main", "origin/master", "main", "master"]
            .into_iter()
            .map(String::from),
    ) {
        let spec = format!("{r}^{{commit}}");
        if run(dir, &["rev-parse", "--verify", "--quiet", &spec])?.0 {
            return Ok(Some(r));
        }
    }
    Ok(None)
}

/// The workspace's repos with their base, after a fetch (a failed one is not an error:
/// the last known base is used).
async fn states(app: &AppState, key: &TaskKey) -> Result<Vec<RepoState>, ApiError> {
    app.store
        .get_task(key)
        .await?
        .ok_or_else(|| crate::store::StoreError::NotFound(key.clone()))?;
    let Some((_, ws)) = app.store.workspace(key).await? else {
        return Err(ApiError::conflict_reason(
            format!("{key} has no workspace"),
            "no-workspace",
        ));
    };
    if !ws.has_branch() {
        return Err(ApiError::conflict_reason(
            format!("{key} has no branch yet"),
            "no-branch",
        ));
    }
    let opts = GitOptions {
        allow_file_protocol: app.config.allow_file_protocol,
    };
    tokio::task::spawn_blocking(move || inspect(&ws, &opts))
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?
}

fn inspect(ws: &Workspace, opts: &GitOptions) -> Result<Vec<RepoState>, ApiError> {
    let deadline = Deadline::after(FETCH_LIMIT);
    let mut out = Vec::new();
    for (label, dir) in ws.repos() {
        // Only for the fetch; its outcome is the same as the one the start had.
        let _ = delilah_git::prepare_base(&dir, &label, opts, deadline);
        let base = resolve_base(&dir)?;
        let behind = match &base {
            Some(base) => {
                let (ok, count, _) = run(&dir, &["rev-list", "--count", &format!("HEAD..{base}")])?;
                if ok { count.parse().unwrap_or(0) } else { 0 }
            }
            None => 0,
        };
        out.push(RepoState {
            label,
            dir,
            branch: ws.branch.clone(),
            base,
            behind,
        });
    }
    Ok(out)
}

fn summary(states: &[RepoState]) -> (Option<String>, u32) {
    let base = states.iter().find_map(|s| s.base.clone());
    (base, states.iter().map(|s| s.behind).sum())
}

pub async fn status(
    State(app): State<AppState>,
    UrlPath(key): UrlPath<String>,
) -> Result<Json<BaseStatus>, ApiError> {
    let states = states(&app, &TaskKey::new(key)).await?;
    let (base, behind_by) = summary(&states);
    Ok(Json(BaseStatus { base, behind_by }))
}

pub async fn update(
    State(app): State<AppState>,
    UrlPath(key): UrlPath<String>,
) -> Result<Json<BranchUpdate>, ApiError> {
    let key = TaskKey::new(key);
    let states = states(&app, &key).await?;
    let (base, merged) = {
        let work = tokio::task::spawn_blocking(move || merge_all(states));
        work.await
            .map_err(|e| ApiError::internal(e.to_string()))??
    };
    if base.is_some() {
        // Up to date now: the warning about a stale base is settled.
        if app
            .store
            .resolve_notices_of(&key, ".", &[KIND_BASE_STALE])
            .await?
            > 0
        {
            app.emit(Topic::Inbox);
        }
    }
    Ok(Json(BranchUpdate { base, merged }))
}

/// Merges each repo's base into its branch; nothing is touched while any repo has
/// tracked changes or is off the task's branch.
fn merge_all(states: Vec<RepoState>) -> Result<(Option<String>, u32), ApiError> {
    for s in &states {
        let (_, on, _) = run(&s.dir, &["symbolic-ref", "--short", "-q", "HEAD"])?;
        if on != s.branch {
            return Err(ApiError::conflict_reason(
                format!("{} is not on {}", location(s), s.branch),
                "not-on-branch",
            ));
        }
        let (_, dirty, _) = run(&s.dir, &["status", "--porcelain", "--untracked-files=no"])?;
        if !dirty.is_empty() {
            return Err(ApiError::conflict_reason(
                format!(
                    "{} has uncommitted changes; commit or discard them before updating the branch",
                    location(s)
                ),
                "workspace-dirty",
            ));
        }
    }
    let mut merged = 0;
    for s in &states {
        let (Some(base), true) = (&s.base, s.behind > 0) else {
            continue;
        };
        let (ok, _, stderr) = run(&s.dir, &["merge", "--no-edit", base])?;
        if !ok {
            let (_, files, _) = run(&s.dir, &["diff", "--name-only", "--diff-filter=U"])?;
            let _ = run(&s.dir, &["merge", "--abort"]);
            return Err(if files.is_empty() {
                ApiError::conflict_reason(
                    format!("could not merge {base} into {}: {stderr}", location(s)),
                    "update-failed",
                )
            } else {
                ApiError::conflict_reason(
                    format!(
                        "merging {base} into {} conflicts in {}; the branch was left as it was",
                        location(s),
                        files.lines().collect::<Vec<_>>().join(", ")
                    ),
                    "update-conflict",
                )
            });
        }
        merged += s.behind;
    }
    Ok((summary(&states).0, merged))
}

fn location(s: &RepoState) -> String {
    if s.label == "." {
        "the workspace".into()
    } else {
        s.label.clone()
    }
}

#[cfg(test)]
mod tests {
    use delilah_core::{Event, Task};

    use super::*;

    fn sh(dir: &Path, args: &[&str]) -> String {
        let out = Command::new("git")
            .args(["-c", "user.name=T", "-c", "user.email=t@d.invalid"])
            .arg("-C")
            .arg(dir)
            .args(args)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    }

    /// A task on `feature/PROJ-1` in a clone whose origin has since got `ahead` new
    /// commits on main (each touching `conflict.txt` when `conflict`).
    struct Fixture {
        app: AppState,
        work: PathBuf,
        key: TaskKey,
        _tmp: tempfile::TempDir,
    }

    async fn fixture(ahead: u32, conflict: bool) -> Fixture {
        let tmp = tempfile::TempDir::new().unwrap();
        let root = tmp.path().canonicalize().unwrap();
        let origin = root.join("origin.git");
        let work = root.join("work");
        let other = root.join("other");
        std::fs::create_dir_all(&origin).unwrap();
        sh(&origin, &["init", "-q", "--bare", "-b", "main"]);
        sh(&root, &["clone", "-q", origin.to_str().unwrap(), "work"]);
        sh(&work, &["checkout", "-q", "-b", "main"]);
        std::fs::write(work.join("conflict.txt"), "base\n").unwrap();
        sh(&work, &["add", "."]);
        sh(&work, &["commit", "-qm", "init"]);
        sh(&work, &["push", "-q", "origin", "main"]);
        sh(&work, &["remote", "set-head", "origin", "main"]);
        sh(&work, &["checkout", "-q", "-b", "feature/PROJ-1"]);
        let theirs = if conflict { "conflict.txt" } else { "mine.txt" };
        std::fs::write(work.join(theirs), "mine\n").unwrap();
        sh(&work, &["add", "."]);
        sh(&work, &["commit", "-qm", "mine"]);

        sh(&root, &["clone", "-q", origin.to_str().unwrap(), "other"]);
        for n in 0..ahead {
            let file = if conflict {
                "conflict.txt".to_string()
            } else {
                format!("n{n}.txt")
            };
            std::fs::write(other.join(file), format!("main {n}\n")).unwrap();
            sh(&other, &["add", "."]);
            sh(&other, &["commit", "-qm", "main moves"]);
        }
        sh(&other, &["push", "-q", "origin", "main"]);

        let config = crate::config::Config {
            home: root.join("home"),
            ..crate::config::Config::test_defaults()
        };
        let app = AppState::new(crate::store::Store::in_memory().await.unwrap(), config);
        let key = TaskKey::new("PROJ-1");
        app.store
            .create_task(&Task::new(key.clone(), "t", "feature"))
            .await
            .unwrap();
        app.store
            .apply_event(&key, Event::Start, chrono::Utc::now())
            .await
            .unwrap();
        let ws = Workspace {
            path: work.clone(),
            branch: "feature/PROJ-1".into(),
            kind: delilah_core::ProjectKind::Repo,
            submodules: vec![],
        };
        app.store.save_workspace(&key, &work, &ws).await.unwrap();
        Fixture {
            app,
            work,
            key,
            _tmp: tmp,
        }
    }

    async fn call_status(f: &Fixture) -> Result<Json<BaseStatus>, ApiError> {
        status(State(f.app.clone()), UrlPath(f.key.to_string())).await
    }

    async fn call_update(f: &Fixture) -> Result<Json<BranchUpdate>, ApiError> {
        update(State(f.app.clone()), UrlPath(f.key.to_string())).await
    }

    #[tokio::test]
    async fn status_fetches_and_counts_the_commits_main_is_ahead() {
        let f = fixture(3, false).await;
        let Ok(Json(s)) = call_status(&f).await else {
            panic!("status failed")
        };
        assert_eq!(
            s,
            BaseStatus {
                base: Some("origin/main".into()),
                behind_by: 3
            }
        );
    }

    #[tokio::test]
    async fn update_merges_the_base_and_settles_the_stale_notice() {
        let f = fixture(2, false).await;
        f.app
            .store
            .insert_notice(
                &f.key,
                &crate::store_prs::NewNotice {
                    repo: ".".into(),
                    kind: KIND_BASE_STALE.into(),
                    cause: "start x".into(),
                    text: "stale".into(),
                    detail: None,
                },
            )
            .await
            .unwrap();
        let Ok(Json(u)) = call_update(&f).await else {
            panic!("update failed")
        };
        assert_eq!(
            u,
            BranchUpdate {
                base: Some("origin/main".into()),
                merged: 2
            }
        );
        assert!(f.work.join("n1.txt").exists() && f.work.join("mine.txt").exists());
        assert_eq!(
            sh(&f.work, &["rev-list", "--count", "HEAD..origin/main"]),
            "0"
        );
        assert!(f.app.store.open_notices().await.unwrap().is_empty());
        let Ok(Json(again)) = call_update(&f).await else {
            panic!("update failed")
        };
        assert_eq!(again.merged, 0, "nothing left to bring");
    }

    #[tokio::test]
    async fn update_refuses_a_workspace_with_changes() {
        let f = fixture(1, false).await;
        std::fs::write(f.work.join("conflict.txt"), "edited\n").unwrap();
        let Err(e) = call_update(&f).await else {
            panic!("must refuse")
        };
        assert_eq!(e.status, axum::http::StatusCode::CONFLICT);
        assert_eq!(e.reason.as_deref(), Some("workspace-dirty"));
        assert!(!f.work.join("n0.txt").exists());
    }

    #[tokio::test]
    async fn update_undoes_a_conflicting_merge() {
        let f = fixture(1, true).await;
        let before = sh(&f.work, &["rev-parse", "HEAD"]);
        let Err(e) = call_update(&f).await else {
            panic!("must refuse")
        };
        assert_eq!(e.status, axum::http::StatusCode::CONFLICT);
        assert_eq!(e.reason.as_deref(), Some("update-conflict"));
        assert!(e.message.contains("conflict.txt"), "{}", e.message);
        assert_eq!(sh(&f.work, &["rev-parse", "HEAD"]), before);
        assert_eq!(sh(&f.work, &["status", "--porcelain"]), "");
    }

    #[tokio::test]
    async fn update_needs_a_workspace_on_its_branch() {
        let f = fixture(1, false).await;
        sh(&f.work, &["checkout", "-q", "main"]);
        let Err(e) = call_update(&f).await else {
            panic!("must refuse")
        };
        assert_eq!(e.reason.as_deref(), Some("not-on-branch"));
        let key = TaskKey::new("PROJ-2");
        f.app
            .store
            .create_task(&Task::new(key.clone(), "t", "feature"))
            .await
            .unwrap();
        let Err(e) = update(State(f.app.clone()), UrlPath(key.to_string())).await else {
            panic!("must refuse")
        };
        assert_eq!(e.reason.as_deref(), Some("no-workspace"));
    }
}
