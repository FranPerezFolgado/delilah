//! Starting a parent task and its subtasks (US1), through the real router with a fake
//! Claude and a fixture repo with a bare origin.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use delilah_core::hierarchy::TaskKind;
use delilah_core::{Project, ProjectKind, TaskKey, TaskStatus, WorkspaceMode};
use delilah_daemon::api::router;
use delilah_daemon::config::{Config, DEFAULT_PR_POLL};
use delilah_daemon::state::AppState;
use delilah_daemon::store::Store;
use http_body_util::BodyExt;
use serde_json::{Value, json};
use tower::ServiceExt;

fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args([
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@t",
            "-c",
            "protocol.file.allow=always",
        ])
        .args(args)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

struct Env {
    app: AppState,
    home: PathBuf,
    origin: PathBuf,
    repo: PathBuf,
}

impl Env {
    async fn new() -> Self {
        let home = std::env::temp_dir().join(format!("delilah-parent-{}", uuid()));
        let origin = home.join("origin.git");
        let repo = home.join("app");
        std::fs::create_dir_all(&repo).unwrap();
        git(&repo, &["init", "-q", "-b", "main"]);
        std::fs::write(repo.join("README.md"), "app\n").unwrap();
        git(&repo, &["add", "-A"]);
        git(&repo, &["commit", "-q", "-m", "init"]);
        git(
            &home,
            &[
                "clone",
                "-q",
                "--bare",
                repo.to_str().unwrap(),
                origin.to_str().unwrap(),
            ],
        );
        git(
            &repo,
            &["remote", "add", "origin", origin.to_str().unwrap()],
        );
        git(&repo, &["fetch", "-q", "origin"]);
        git(
            &repo,
            &["branch", "-q", "--set-upstream-to=origin/main", "main"],
        );
        git(&repo, &["remote", "set-head", "origin", "main"]);
        let env = Self::on(
            &home,
            origin,
            repo.clone(),
            Store::open(&home.join("delilah.db")).await.unwrap(),
        );
        let project = Project {
            name: "app".into(),
            path: repo,
            kind: ProjectKind::Repo,
            workflow: "project".into(),
            settings: delilah_core::ProjectSettings {
                // Kinds must ignore this: they never work in the checkout.
                workspace: Some(WorkspaceMode::Checkout),
                ..Default::default()
            },
            preview_command: None,
        };
        env.app.store.create_project(&project).await.unwrap();
        env
    }

    fn on(home: &Path, origin: PathBuf, repo: PathBuf, store: Store) -> Self {
        // Stands in for `claude`: stays up until the test ends.
        let fake = home.join("fake-claude.sh");
        if !fake.exists() {
            std::fs::write(&fake, "#!/bin/sh\nsleep 30\n").unwrap();
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        let config = Config {
            home: home.to_path_buf(),
            addr: "127.0.0.1:7420".parse().unwrap(),
            allow_file_protocol: true,
            notify: false,
            ui_origin: None,
            claude_bin: "claude".into(),
            agent_program: Some(fake),
            claude_projects: home.join("claude-projects"),
            gh_program: None,
            secrets_in_memory: true,
            jira_http_loopback: false,
            pr_poll: DEFAULT_PR_POLL,
        };
        Self {
            app: AppState::new(store, config),
            home: home.to_path_buf(),
            origin,
            repo,
        }
    }

    fn router(&self) -> Router {
        router(self.app.clone())
    }

    async fn call(&self, method: &str, path: &str, body: Value) -> (StatusCode, Value) {
        let req = Request::builder()
            .method(method)
            .uri(path)
            .header("content-type", "application/json")
            .body(Body::from(body.to_string()))
            .unwrap();
        let res = self.router().oneshot(req).await.unwrap();
        let status = res.status();
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        (
            status,
            serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        )
    }

    /// Creates a backlog task of a kind and returns its key.
    async fn create(&self, kind: Option<&str>, parent: Option<&str>) -> String {
        let (status, task) = self
            .call(
                "POST",
                "/api/v1/tasks",
                json!({ "title": "work", "project": "app", "free": true,
                        "kind": kind, "parent_task": parent }),
            )
            .await;
        assert_eq!(status, StatusCode::CREATED, "{task}");
        task["key"].as_str().unwrap().to_string()
    }

    async fn start(&self, key: &str) -> (StatusCode, Value) {
        let branch = format!("feature/{}", key.to_lowercase());
        self.call(
            "POST",
            "/api/v1/runs",
            json!({ "key": key, "free": true, "branch": branch }),
        )
        .await
    }

    async fn status(&self, key: &str) -> TaskStatus {
        self.app
            .store
            .get_task(&TaskKey::new(key))
            .await
            .unwrap()
            .unwrap()
            .status
    }

    async fn workspace(&self, key: &str) -> delilah_git::Workspace {
        self.app
            .store
            .workspace(&TaskKey::new(key))
            .await
            .unwrap()
            .unwrap()
            .1
    }

    fn origin_has(&self, branch: &str) -> bool {
        Command::new("git")
            .arg("-C")
            .arg(&self.origin)
            .args([
                "rev-parse",
                "--verify",
                "-q",
                &format!("refs/heads/{branch}"),
            ])
            .output()
            .unwrap()
            .status
            .success()
    }

    async fn end(&self) {
        // Stops the fake agents that are still up.
        for record in self.app.store.open_sessions(None).await.unwrap() {
            if let Some(live) = self.app.sessions.get(&record.id) {
                let _ = live.runtime.kill();
            }
        }
        std::fs::remove_dir_all(&self.home).ok();
    }
}

fn uuid() -> String {
    static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    format!("{}-{n}", std::process::id())
}

#[tokio::test]
async fn a_parent_task_gets_a_worktree_and_a_pushed_branch_and_no_agent() {
    let env = Env::new().await;
    let key = env.create(Some("parent_task"), None).await;
    let (status, task) = env.start(&key).await;
    assert_eq!(status, StatusCode::CREATED, "{task}");
    assert_eq!(env.status(&key).await, TaskStatus::Running);

    // A worktree of its own, although the project's mode is the checkout.
    let ws = env.workspace(&key).await;
    assert!(ws.path.join(".git").is_file(), "a linked worktree");
    assert_ne!(ws.path, env.repo);
    assert_eq!(git(&ws.path, &["branch", "--show-current"]), ws.branch);
    assert!(env.origin_has(&ws.branch), "the branch is on the remote");
    // The user's checkout is untouched.
    assert_eq!(git(&env.repo, &["branch", "--show-current"]), "main");

    // No agent: no session at all.
    assert!(
        env.app
            .store
            .open_sessions(Some(&key))
            .await
            .unwrap()
            .is_empty()
    );
    assert!(env.app.sessions.of_task(&key).is_empty());
    env.end().await;
}

#[tokio::test]
async fn a_parent_task_stays_running_across_a_restart_and_an_extra_session_ending() {
    let env = Env::new().await;
    let key = env.create(Some("parent_task"), None).await;
    assert_eq!(env.start(&key).await.0, StatusCode::CREATED);

    // A new "+ Claude" session that ends does not move the task.
    let (status, session) = env
        .call(
            "POST",
            &format!("/api/v1/tasks/{key}/sessions"),
            json!({ "kind": "claude" }),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{session}");
    let id = session["id"].as_str().unwrap();
    let (status, _) = env
        .call("POST", &format!("/api/v1/sessions/{id}/close"), json!({}))
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert_eq!(env.status(&key).await, TaskStatus::Running);

    // The daemon restarts: same database, nothing live, every startup recovery runs.
    let restarted = Env::on(
        &env.home,
        env.origin.clone(),
        env.repo.clone(),
        Store::open(&env.home.join("delilah.db")).await.unwrap(),
    );
    delilah_daemon::scheduler::recover_branches(&restarted.app).await;
    delilah_daemon::recover_interrupted_setups(&restarted.app)
        .await
        .unwrap();
    delilah_daemon::permissions::reconcile_lost(&restarted.app)
        .await
        .unwrap();
    delilah_daemon::scheduler::tick(&restarted.app).await;
    assert_eq!(restarted.status(&key).await, TaskStatus::Running);
    env.end().await;
}

#[tokio::test]
async fn a_reopened_parent_task_and_subtask_keep_their_kind_and_link() {
    let env = Env::new().await;
    let parent = env.create(Some("parent_task"), None).await;
    assert_eq!(env.start(&parent).await.0, StatusCode::CREATED);
    let sub = env.create(None, Some(&parent)).await;
    let (status, body) = env.start(&sub).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");

    for key in [&sub, &parent] {
        assert_eq!(
            env.call("POST", &format!("/api/v1/tasks/{key}/done"), json!({}))
                .await
                .0,
            StatusCode::OK
        );
        assert_eq!(env.status(key).await, TaskStatus::Done);
        let (status, body) = env
            .call("POST", &format!("/api/v1/tasks/{key}/reopen"), json!({}))
            .await;
        assert_eq!(status, StatusCode::OK, "{body}");
    }
    let p = env
        .app
        .store
        .get_task(&TaskKey::new(&parent))
        .await
        .unwrap()
        .unwrap();
    let s = env
        .app
        .store
        .get_task(&TaskKey::new(&sub))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(p.kind, TaskKind::ParentTask);
    assert_eq!(s.kind, TaskKind::Subtask);
    assert_eq!(s.parent_task, Some(TaskKey::new(&parent)));
    env.end().await;
}

#[tokio::test]
async fn a_subtask_cannot_start_before_its_parent_task() {
    let env = Env::new().await;
    let parent = env.create(Some("parent_task"), None).await;
    let sub = env.create(None, Some(&parent)).await;
    let (status, body) = env.start(&sub).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["error"], "parent-task-not-started");
    assert_eq!(body["key"], parent.as_str());
    // Nothing was created: it is still in the backlog.
    assert_eq!(env.status(&sub).await, TaskStatus::Queued);
    assert!(
        env.app
            .store
            .workspace(&TaskKey::new(&sub))
            .await
            .unwrap()
            .is_none()
    );
    env.end().await;
}

#[tokio::test]
async fn subtasks_start_in_parallel_from_the_parent_task_branch() {
    let env = Env::new().await;
    let parent = env.create(Some("parent_task"), None).await;
    assert_eq!(env.start(&parent).await.0, StatusCode::CREATED);
    let branch = env.workspace(&parent).await.branch;

    // Work on the parent task's branch (its push also updates `origin/<branch>`, so the
    // starts below see it without a new fetch).
    let parent_ws = env.workspace(&parent).await;
    git(
        &parent_ws.path,
        &[
            "commit",
            "-q",
            "--allow-empty",
            "-m",
            "marker on the parent task",
        ],
    );
    git(&parent_ws.path, &["push", "-q", "origin", &branch]);

    let (a, b) = (
        env.create(None, Some(&parent)).await,
        env.create(None, Some(&parent)).await,
    );
    let ((sa, ba), (sb, bb)) = tokio::time::timeout(Duration::from_secs(120), async {
        tokio::join!(env.start(&a), env.start(&b))
    })
    .await
    .unwrap();
    assert_eq!(sa, StatusCode::CREATED, "{ba}");
    assert_eq!(sb, StatusCode::CREATED, "{bb}");
    for key in [&a, &b] {
        let ws = env.workspace(key).await;
        assert_ne!(ws.branch, branch);
        assert!(git(&ws.path, &["log", "--format=%s"]).contains("marker on the parent task"));
        assert_eq!(env.status(key).await, TaskStatus::Running);
    }
    env.end().await;
}

#[tokio::test]
async fn a_missing_remote_parent_branch_refuses_the_subtask_start() {
    let env = Env::new().await;
    let parent = env.create(Some("parent_task"), None).await;
    assert_eq!(env.start(&parent).await.0, StatusCode::CREATED);
    let branch = env.workspace(&parent).await.branch;
    git(&env.origin, &["branch", "-q", "-D", &branch]);
    // The local remote-tracking ref is gone after the next fetch's prune; remove it now.
    git(&env.repo, &["fetch", "-q", "--prune", "origin"]);

    let sub = env.create(None, Some(&parent)).await;
    let (status, body) = env.start(&sub).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["error"], "parent-task-branch-missing");
    assert_eq!(body["branch"], branch.as_str());
    assert_ne!(env.status(&sub).await, TaskStatus::Running);
    env.end().await;
}

#[tokio::test]
async fn a_subtask_takes_the_parent_task_submodules_in_a_parent_project() {
    let home = std::env::temp_dir().join(format!("delilah-parent-sub-{}", uuid()));
    let fixture = Command::new("bash")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../scripts/make-fixture-parent.sh"
        ))
        .arg(home.join("env"))
        .output()
        .unwrap();
    assert!(fixture.status.success());
    let checkout = home.join("env/parent");
    let env = Env::on(
        &home,
        home.join("env/remotes/parent.git"),
        checkout.clone(),
        Store::open(&home.join("delilah.db")).await.unwrap(),
    );
    env.app
        .store
        .create_project(&Project {
            name: "app".into(),
            path: checkout,
            kind: ProjectKind::Parent,
            workflow: "project".into(),
            settings: Default::default(),
            preview_command: None,
        })
        .await
        .unwrap();
    let (status, task) = env
        .call(
            "POST",
            "/api/v1/tasks",
            json!({ "title": "p", "project": "app", "free": true, "kind": "parent_task" }),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{task}");
    let parent = task["key"].as_str().unwrap().to_string();
    let (status, body) = env
        .call("POST", "/api/v1/runs", json!({ "key": parent, "free": true, "branch": "feature/p", "submodules": ["svc-orders"] }))
        .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");

    // A submodule push lost after the start is repaired by `push-branch`.
    let pws = env.workspace(&parent).await;
    let remote = git(
        &pws.path.join("svc-orders"),
        &["remote", "get-url", "origin"],
    );
    let remote = std::path::PathBuf::from(remote.trim());
    let remote = if remote.is_relative() {
        pws.path.join("svc-orders").join(remote)
    } else {
        remote
    };
    git(&remote, &["branch", "-D", &pws.branch]);
    let (status, body) = env
        .call(
            "POST",
            &format!("/api/v1/tasks/{parent}/push-branch"),
            json!({}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    git(&remote, &["rev-parse", "--verify", &pws.branch]);

    let sub = env.create(None, Some(&parent)).await;
    let (status, body) = env.start(&sub).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let ws = env.workspace(&sub).await;
    assert_eq!(ws.submodules, ["svc-orders"]);
    assert_eq!(
        git(&ws.path.join("svc-orders"), &["branch", "--show-current"]),
        ws.branch
    );
    env.end().await;
}
