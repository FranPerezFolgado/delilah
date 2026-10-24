//! Helpers shared by the tests of the autopilot's modules.

use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use chrono::Utc;
use delilah_core::{Autopilot, AutopilotStage, AutopilotState, Step, Task, TaskKey};

use super::outcome_path;
use super::recover;
use crate::api::ApiError;
use crate::gh::fake::{FakeGh, PR_URL, REPO_VIEW, bare_snapshot};
use crate::pr_testkit::{TempDir, git_repo_with_origin, task_with_workspace};
use crate::state::AppState;
use crate::store_sessions::{SessionKind, SessionRecord};

pub(crate) fn running() -> Autopilot {
    Autopilot {
        state: AutopilotState::Running,
        stage: None,
        reason: None,
        since: Utc::now(),
        nudged: None,
        review_session: None,
    }
}

pub(crate) fn task_with(autopilot: Option<AutopilotState>) -> Task {
    let mut t = Task::new(TaskKey::new("PROJ-1"), "t", "feature");
    t.saving_mode = true;
    t.autopilot = autopilot.map(|state| Autopilot { state, ..running() });
    t
}

pub(crate) fn gh(list: &str) -> Arc<FakeGh> {
    Arc::new(
        FakeGh::new()
            .with_user("me")
            .on(&["repo", "view"], Ok(REPO_VIEW))
            .on(&["pr", "list"], Ok(list))
            .on(&["pr", "create"], Ok(PR_URL))
            .on(&["api", "graphql"], Ok(&bare_snapshot("OPEN", false))),
    )
}

/// A task on autopilot with a review session `rev-1`, whose branch has one commit
/// beyond `main`.
pub(crate) async fn ready_task(
    gh: Arc<FakeGh>,
    stage: Option<AutopilotStage>,
) -> (AppState, TaskKey, TempDir) {
    let (app, key, dir) = task_with_workspace(gh, None).await;
    git_repo_with_origin(
        &dir.path().join("ws"),
        &dir.path().join("origin"),
        "feature/PROJ-1",
        &["first"],
    );
    let a = Autopilot {
        stage,
        review_session: Some("rev-1".into()),
        ..running()
    };
    app.store.set_autopilot(&key, Some(&a)).await.unwrap();
    app.store
        .create_session(&SessionRecord {
            id: "rev-1".into(),
            task: "PROJ-1".into(),
            kind: SessionKind::Claude,
            label: "Revisión".into(),
            primary: true,
            closed: false,
            read_only: false,
            conversation: false,
            step: Some(Step::Review),
        })
        .await
        .unwrap();
    (app, key, dir)
}

pub(crate) fn write_outcome(app: &AppState, key: &TaskKey, content: &str) {
    let path = outcome_path(app, key, "rev-1");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, content).unwrap();
}

pub(crate) async fn autopilot_of(app: &AppState, key: &TaskKey) -> Autopilot {
    app.store
        .get_task(key)
        .await
        .unwrap()
        .unwrap()
        .autopilot
        .unwrap()
}

/// Runs `recover` and waits for the finish it spawned, if any (and for its pause).
pub(crate) async fn recover_and_wait(app: &AppState, task: &Task) {
    recover(app, task);
    for _ in 0..400 {
        let running = app
            .finishing
            .running
            .lock()
            .unwrap()
            .contains(task.key.as_str());
        if !running {
            // The finish lets go of its guard before it pauses.
            tokio::time::sleep(Duration::from_millis(100)).await;
            return;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("the recovery never ended");
}

pub(crate) fn creates(gh: &FakeGh) -> Vec<Vec<String>> {
    gh.calls()
        .into_iter()
        .filter(|c| c.starts_with(&["pr".to_string(), "create".to_string()]))
        .collect()
}

pub(crate) async fn with_preview(app: &mut AppState, command: &str, timeout_ms: u64) {
    app.store
        .set_preview_command("proj", Some(command))
        .await
        .unwrap();
    app.previews = Arc::new(crate::preview::Previews::with_timeout(
        crate::events::Events::default(),
        std::time::Duration::from_millis(timeout_ms),
    ));
}

/// A ready task whose repo has `subjects` commits.
pub(crate) async fn task_with_commits(
    gh: Arc<FakeGh>,
    stage: Option<AutopilotStage>,
    subjects: &[&str],
) -> (AppState, TaskKey, TempDir) {
    let (app, key, dir) = task_with_workspace(gh, None).await;
    git_repo_with_origin(
        &dir.path().join("ws"),
        &dir.path().join("origin"),
        "feature/PROJ-1",
        subjects,
    );
    let a = Autopilot {
        stage,
        review_session: Some("rev-1".into()),
        ..running()
    };
    app.store.set_autopilot(&key, Some(&a)).await.unwrap();
    (app, key, dir)
}

pub(crate) async fn stage_of(app: &AppState, key: &TaskKey) -> Option<AutopilotStage> {
    autopilot_of(app, key).await.stage
}

pub(crate) type Launched = Mutex<Vec<(SessionRecord, Option<String>)>>;

pub(crate) fn stub(
    launched: &Launched,
) -> impl Fn(&AppState, &SessionRecord, &Path, Option<&str>, bool) -> Result<(), ApiError> + '_ {
    move |_, s, _, prompt, _| {
        launched
            .lock()
            .unwrap()
            .push((s.clone(), prompt.map(str::to_string)));
        Ok(())
    }
}

pub(crate) fn paused() -> Autopilot {
    Autopilot {
        state: AutopilotState::Paused,
        reason: Some("x".into()),
        nudged: Some(Step::Plan),
        ..running()
    }
}

pub(crate) fn git_in(dir: &Path, args: &[&str]) {
    let out = std::process::Command::new("git")
        .args(["-c", "user.name=t", "-c", "user.email=t@t"])
        .args(["-c", "commit.gpgsign=false"])
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap();
    assert!(out.status.success(), "git {args:?}");
}

/// Writes (and optionally commits) the spec's `tasks.md` in the workspace.
pub(crate) fn write_spec(app: &AppState, tasks: &str, commit: bool) {
    let ws = app.config.home.parent().unwrap().join("ws");
    std::fs::create_dir_all(ws.join("specs/proj-1-x")).unwrap();
    std::fs::write(ws.join("specs/proj-1-x/tasks.md"), tasks).unwrap();
    if commit {
        git_in(&ws, &["add", "."]);
        git_in(&ws, &["commit", "-m", "spec"]);
    }
}

pub(crate) async fn paused_task(
    gh: Arc<FakeGh>,
    stage: Option<AutopilotStage>,
) -> (AppState, TaskKey, TempDir) {
    let (app, key, dir) = task_with_commits(gh, stage, &["first"]).await;
    app.store.set_saving_mode(&key, true).await.unwrap();
    let a = Autopilot {
        stage,
        review_session: Some("rev-1".into()),
        ..paused()
    };
    app.store.set_autopilot(&key, Some(&a)).await.unwrap();
    (app, key, dir)
}

pub(crate) async fn session(app: &AppState, id: &str, step: Step) {
    app.store
        .create_session(&SessionRecord {
            id: id.into(),
            task: "PROJ-1".into(),
            kind: SessionKind::Claude,
            label: "s".into(),
            primary: true,
            closed: false,
            read_only: false,
            conversation: false,
            step: Some(step),
        })
        .await
        .unwrap();
}

pub(crate) fn event_names(events: &[crate::store::EventRecord]) -> Vec<&'static str> {
    events.iter().map(|e| e.event.name()).collect()
}

pub(crate) async fn wait_for(app: &AppState, key: &TaskKey, state: AutopilotState) {
    for _ in 0..400 {
        if autopilot_of(app, key).await.state == state {
            return;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("the autopilot never became {state:?}");
}

/// A parent task with repos `svc-a` and `svc-b` that both have commits, ready to
/// open, with `svc-b`'s origin removed so pushing there fails.
pub(crate) async fn parent_ready(gh: Arc<FakeGh>) -> (AppState, TaskKey, TempDir) {
    let (app, key, dir) = task_with_workspace(gh, None).await;
    let ws_path = dir.path().join("ws");
    for sub in ["svc-a", "svc-b"] {
        git_repo_with_origin(
            &ws_path.join(sub),
            &dir.path().join(format!("origin-{sub}")),
            "feature/PROJ-1",
            &[sub],
        );
    }
    let ws = delilah_git::Workspace {
        path: ws_path.clone(),
        branch: "feature/PROJ-1".into(),
        kind: delilah_core::ProjectKind::Parent,
        submodules: vec!["svc-a".into(), "svc-b".into()],
    };
    app.store
        .save_workspace(&key, Path::new("/parent"), &ws)
        .await
        .unwrap();
    let a = Autopilot {
        review_session: Some("rev-1".into()),
        ..running()
    };
    app.store.set_autopilot(&key, Some(&a)).await.unwrap();
    git_in(&ws_path.join("svc-b"), &["remote", "remove", "origin"]);
    write_outcome(&app, &key, r#"{"outcome":"ready"}"#);
    (app, key, dir)
}
