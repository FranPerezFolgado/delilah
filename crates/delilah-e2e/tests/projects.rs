//! US2-1 and US2-11: projects registered through the CLI, and surviving a daemon restart.

use std::time::Duration;

use delilah_e2e::api::items;
use delilah_e2e::{Env, EnvOptions};
use serde_json::{Value, json};

const T: Duration = Duration::from_secs(30);

fn kind_of(projects: &Value, name: &str) -> Option<String> {
    items(projects)
        .iter()
        .find(|p| p["name"] == name)
        .map(|p| p["kind"].as_str().unwrap_or("").to_string())
}

#[test]
fn project_add_through_the_cli_lists_repo_and_parent() {
    let env = Env::start(EnvOptions::default());
    for (path, kind, name) in [
        (&env.fixtures.repo, "repo", "signal"),
        (&env.fixtures.parent, "parent", "parent"),
    ] {
        let (status, out, err) = env.cli().run(&[
            "project",
            "add",
            path.to_str().unwrap(),
            "--kind",
            kind,
            "--name",
            name,
        ]);
        assert!(status.success(), "project add {kind}: {out}\n{err}");
    }
    let projects = env.api().projects();
    assert_eq!(
        kind_of(&projects, "signal").as_deref(),
        Some("repo"),
        "{projects}"
    );
    assert_eq!(
        kind_of(&projects, "parent").as_deref(),
        Some("parent"),
        "{projects}"
    );
    let (status, out, err) = env.cli().run(&["project", "ls"]);
    assert!(status.success(), "project ls: {out}\n{err}");
    assert!(out.contains("signal") && out.contains("parent"), "{out}");
    // On disk: the fixtures are still the git repos that were registered.
    assert!(env.fixtures.repo.join(".git").exists());
    assert!(env.fixtures.parent.join(".gitmodules").is_file());
}

#[test]
fn projects_tasks_and_sessions_survive_a_daemon_restart() {
    let mut env = Env::start(EnvOptions::default());
    let (repo, _parent) = env.register_fixtures();
    env.api().start_run(json!({
        "key": "E2E-11", "title": "Restart", "project": repo, "prompt": "Restart", "submodules": [],
    }));
    let session = env.wait_for("the task's session", T, || {
        let all = env.api().sessions();
        match items(&all)
            .iter()
            .find(|s| s["task"] == "E2E-11" && s["primary"] == true)
        {
            Some(s) => Ok(s["id"].as_str().unwrap().to_string()),
            None => Err(all),
        }
    });
    let port = env.port;
    env.restart_daemon();
    assert_eq!(env.port, port, "same port after the restart");

    let projects = env.api().projects();
    assert_eq!(
        kind_of(&projects, "signal").as_deref(),
        Some("repo"),
        "{projects}"
    );
    assert_eq!(
        kind_of(&projects, "parent").as_deref(),
        Some("parent"),
        "{projects}"
    );
    let task = env.api().task("E2E-11");
    assert_eq!(task["key"], "E2E-11", "{task}");
    let all = env.api().sessions();
    assert!(
        items(&all).iter().any(|s| s["id"] == session.as_str()),
        "session {session} lost: {all}"
    );
    // The checkout is the default workspace (decision 57): no worktree is created.
    assert!(
        !env.home
            .parent()
            .unwrap()
            .join("delilah/workspaces/E2E-11")
            .exists()
    );
}
