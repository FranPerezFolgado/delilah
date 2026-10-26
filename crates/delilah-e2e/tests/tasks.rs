//! US2-2, US2-3, US2-4 and US2-9: starting tasks in a worktree, with submodules, in the
//! checkout, and deleting them. Each flow checks the daemon's answer and the disk.

use std::path::Path;
use std::process::Command;
use std::time::Duration;

use delilah_e2e::api::items;
use delilah_e2e::{Env, EnvOptions};
use serde_json::{Value, json};

const T: Duration = Duration::from_secs(30);

fn git(env: &Env, dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .env_clear()
        .env("HOME", &env.home)
        .env("PATH", std::env::var("PATH").unwrap_or_default())
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .args(["-C", dir.to_str().unwrap()])
        .args(args)
        .output()
        .unwrap_or_else(|e| panic!("git {args:?}: {e}"));
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn status_type(task: &Value) -> String {
    task["status"]["type"].as_str().unwrap_or("").to_string()
}

fn ws(env: &Env, key: &str) -> std::path::PathBuf {
    env.root.join("delilah/workspaces").join(key)
}

/// Waits until the task's primary session is live and the task is running or waiting.
fn wait_working(env: &Env, key: &str) {
    env.wait_for(&format!("{key} working"), T, || {
        let task = env.api().task(key);
        let all = env.api().sessions();
        let live = items(&all)
            .iter()
            .any(|s| s["task"] == key && s["primary"] == true && s["live"] == true);
        match status_type(&task).as_str() {
            "running" | "waiting-for-user" if live => Ok(()),
            _ => Err((task["status"].clone(), all)),
        }
    });
}

/// The branch the daemon named for the task (`feature/<KEY>-<title slug>` here, since
/// the one-shot Claude fake always fails), as the API reports it.
fn branch_of(env: &Env, key: &str) -> String {
    let task = env.api().task(key);
    let branch = task["branch"]
        .as_str()
        .unwrap_or_else(|| panic!("no branch: {task}"))
        .to_string();
    assert!(branch.starts_with(&format!("feature/{key}")), "{branch}");
    branch
}

#[test]
fn worktree_mode_creates_the_workspace_on_the_feature_branch() {
    let env = Env::start(EnvOptions::default());
    let (repo, _) = env.register_fixtures();
    let (status, out, err) =
        env.cli()
            .run(&["new", "E2E-20", "--title", "Worktree", "--project", &repo]);
    assert!(status.success(), "new: {out}\n{err}");
    let (status, out, err) = env
        .cli()
        .run(&["run", "E2E-20", "--project", &repo, "--worktree"]);
    assert!(status.success(), "run: {out}\n{err}");
    wait_working(&env, "E2E-20");

    let list = git(
        &env,
        &env.fixtures.repo,
        &["worktree", "list", "--porcelain"],
    );
    let want = std::fs::canonicalize(ws(&env, "E2E-20")).unwrap();
    let block = list
        .split("\n\n")
        .find(|b| {
            b.lines()
                .next()
                .is_some_and(|l| Path::new(l.trim_start_matches("worktree ")) == want)
        })
        .unwrap_or_else(|| panic!("no worktree {} in\n{list}", want.display()));
    let branch = branch_of(&env, "E2E-20");
    assert!(
        block.contains(&format!("branch refs/heads/{branch}")),
        "{block}"
    );
    assert_eq!(git(&env, &want, &["branch", "--show-current"]), branch);

    let (status, out, err) = env.cli().run(&["ls"]);
    assert!(status.success(), "ls: {out}\n{err}");
    assert!(out.contains("E2E-20"), "{out}");
}

#[test]
fn a_parent_task_initialises_only_the_chosen_submodule() {
    let env = Env::start(EnvOptions::default());
    let (_, parent) = env.register_fixtures();
    env.api().start_run(json!({
        "key": "E2E-30", "title": "Parent", "project": parent, "prompt": "Parent",
        "submodules": ["svc-orders"], "in_checkout": false,
    }));
    wait_working(&env, "E2E-30");
    let w = ws(&env, "E2E-30");
    let status = git(&env, &w, &["submodule", "status"]);
    let line = |name: &str| {
        status
            .lines()
            .find(|l| l.contains(name))
            .unwrap_or_else(|| panic!("no {name} in\n{status}"))
            .to_string()
    };
    assert!(!line("svc-orders").starts_with('-'), "{status}");
    for other in ["svc-payments", "svc-billing", "common-lib"] {
        assert!(
            line(other).starts_with('-'),
            "{other} should not be initialised:\n{status}"
        );
    }
    // The parent's worktree is detached; the branch lives in the chosen submodule.
    assert_eq!(git(&env, &w, &["branch", "--show-current"]), "");
    assert_eq!(
        git(&env, &w.join("svc-orders"), &["branch", "--show-current"]),
        branch_of(&env, "E2E-30")
    );
}

#[test]
fn checkout_mode_switches_the_fixture_and_archive_restores_it() {
    let env = Env::start(EnvOptions::default());
    let (repo, _) = env.register_fixtures();
    let before = git(&env, &env.fixtures.repo, &["branch", "--show-current"]);
    env.api().start_run(json!({
        "key": "E2E-40", "title": "Checkout", "project": repo, "prompt": "Checkout",
        "submodules": [], "in_checkout": true,
    }));
    wait_working(&env, "E2E-40");
    assert_eq!(
        git(&env, &env.fixtures.repo, &["branch", "--show-current"]),
        branch_of(&env, "E2E-40")
    );

    let (status, out, err) = env.cli().run(&["done", "E2E-40"]);
    assert!(status.success(), "done: {out}\n{err}");
    let (status, out, err) = env.cli().run(&["archive", "E2E-40", "--force"]);
    assert!(status.success(), "archive: {out}\n{err}");
    env.wait_for("the checkout back on its branch", T, || {
        let now = git(&env, &env.fixtures.repo, &["branch", "--show-current"]);
        if now == before { Ok(()) } else { Err(now) }
    });
}

#[test]
fn deleting_a_backlog_task_removes_it_and_its_folder() {
    let env = Env::start(EnvOptions::default());
    let (repo, _) = env.register_fixtures();
    for key in ["E2E-50", "E2E-51"] {
        env.api()
            .new_task(json!({"key": key, "title": "Backlog", "project": repo}));
        let tasks = env.api().tasks();
        assert!(items(&tasks).iter().any(|t| t["key"] == key), "{tasks}");
    }
    let task_dir = |k: &str| env.root.join("delilah/tasks").join(k);

    env.api().delete_task("E2E-50");
    let tasks = env.api().tasks();
    assert!(
        !items(&tasks).iter().any(|t| t["key"] == "E2E-50"),
        "{tasks}"
    );
    assert!(!task_dir("E2E-50").exists());
    assert!(!ws(&env, "E2E-50").exists());

    let (status, out, err) = env.cli().run(&["rm", "E2E-51"]);
    assert!(status.success(), "rm: {out}\n{err}");
    let tasks = env.api().tasks();
    assert!(
        !items(&tasks).iter().any(|t| t["key"] == "E2E-51"),
        "{tasks}"
    );
    assert!(!task_dir("E2E-51").exists());
}
