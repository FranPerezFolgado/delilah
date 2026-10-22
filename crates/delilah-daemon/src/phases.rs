//! A task's phase (specify → plan → implement → review). Three things move it:
//! - the user, to any phase ("Revisar", or the phase bar);
//! - SpecKit's files and the agent's actions (`specs.rs`, `hooks.rs`), only ever
//!   forward, so they never undo what the user chose;
//! - a review hint: when the agent stops with commits on its branch, the inbox asks
//!   whether to move to review rather than guessing.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use delilah_core::{Event, SpecPhase, Task, TaskKey, TaskStatus};
use serde::Deserialize;

use crate::api::ApiError;
use crate::events::Topic;
use crate::state::AppState;

fn rank(phase: Option<&str>) -> Option<SpecPhase> {
    phase.and_then(SpecPhase::parse)
}

fn under_way(task: &Task) -> bool {
    matches!(
        task.status,
        TaskStatus::Running | TaskStatus::WaitingForUser | TaskStatus::PausedByLimit { .. }
    )
}

async fn record(app: &AppState, key: &TaskKey, phase: SpecPhase) -> Result<(), ApiError> {
    let event = Event::PhaseReached {
        phase: phase.name().into(),
    };
    app.transition(key, event).await?;
    if phase == SpecPhase::Review {
        drop_hint(app, key.as_str());
    }
    Ok(())
}

/// Moves the task forward to `phase`; a no-op if it is already there or beyond.
pub async fn advance(app: &AppState, key: &TaskKey, phase: SpecPhase) {
    let Ok(Some(task)) = app.store.get_task(key).await else {
        return;
    };
    if !under_way(&task) || rank(task.phase.as_deref()).is_some_and(|now| now >= phase) {
        return;
    }
    if let Err(e) = record(app, key, phase).await {
        tracing::warn!(%key, "could not move the phase: {}", e.message());
    }
}

#[derive(Debug, Deserialize)]
pub struct SetPhase {
    pub phase: String,
}

/// `POST /api/v1/tasks/{key}/phase`: the user puts the task in a phase, any of them.
pub async fn set(
    State(app): State<AppState>,
    Path(key): Path<String>,
    Json(req): Json<SetPhase>,
) -> Result<Json<Task>, ApiError> {
    let phase = SpecPhase::parse(&req.phase)
        .ok_or_else(|| ApiError::bad_request(format!("unknown phase `{}`", req.phase)))?;
    let key = TaskKey::new(key);
    let before = app.store.get_task(&key).await?.and_then(|t| t.phase);
    record(&app, &key, phase).await?;
    // Only a move forward to implement can offer a new session; going back never does.
    if phase == SpecPhase::Implement && rank(before.as_deref()).is_none_or(|p| p < phase) {
        crate::handoff::on_implement_chosen(&app, &key).await;
    }
    Ok(Json(app.store.get_task(&key).await?.expect("task exists")))
}

/// Suggests reviewing a task (its agent stopped with commits on its branch).
pub fn hint(app: &AppState, task: &str) {
    if app.review_hints.lock().unwrap().insert(task.to_string()) {
        app.emit(Topic::Inbox);
    }
}

pub fn drop_hint(app: &AppState, task: &str) {
    if app.review_hints.lock().unwrap().remove(task) {
        app.emit(Topic::Inbox);
    }
}

/// `POST /api/v1/tasks/{key}/review-hint/dismiss`: "Todavía no".
pub async fn dismiss(State(app): State<AppState>, Path(key): Path<String>) -> StatusCode {
    drop_hint(&app, &key);
    StatusCode::NO_CONTENT
}

/// After a turn of a task's agent: if the branch has commits and the task is not in
/// review yet, ask (in the inbox) whether to move it there.
pub async fn maybe_hint(app: &AppState, key: &TaskKey) {
    let Ok(Some(task)) = app.store.get_task(key).await else {
        return;
    };
    if !under_way(&task) || rank(task.phase.as_deref()).is_some_and(|p| p >= SpecPhase::Review) {
        return;
    }
    let Ok(ws) = crate::owners::workspace(app, key.as_str()).await else {
        return;
    };
    let opts = delilah_git::GitOptions {
        allow_file_protocol: app.config.allow_file_protocol,
    };
    let commits = tokio::task::spawn_blocking(move || {
        ws.repos()
            .iter()
            .filter_map(|(_, dir)| delilah_git::changes(dir, &opts).ok())
            .map(|c| c.commits)
            .sum::<u32>()
    })
    .await
    .unwrap_or(0);
    if commits > 0 {
        hint(app, key.as_str());
    }
}

/// What the agent just did that says where the task is. `file` paths are as Claude
/// reports them (absolute): edits inside the spec folders are SpecKit's own work.
pub fn signal_phase(hook: &serde_json::Value) -> Option<SpecPhase> {
    let tool = hook["tool_name"].as_str()?;
    let input = &hook["tool_input"];
    match tool {
        "Edit" | "Write" | "MultiEdit" | "NotebookEdit" => {
            let file = input["file_path"]
                .as_str()
                .or(input["notebook_path"].as_str())?;
            let spec_work = file.contains("/specs/") || file.contains("/.specify/");
            (!spec_work).then_some(SpecPhase::Implement)
        }
        "Bash" => {
            let command = input["command"].as_str()?;
            // Opening a PR or pushing: the agent considers the work ready.
            let ready = command.contains("gh pr create") || command.contains("git push");
            ready.then_some(SpecPhase::Review)
        }
        _ => None,
    }
}

/// Reacts to a hook of one of a task's sessions (not a project's).
pub async fn on_hook(
    app: &AppState,
    task: &str,
    signal: delilah_agents::claude::Signal,
    hook: &serde_json::Value,
    primary: bool,
    limited: bool,
) {
    use delilah_agents::claude::Signal;
    let key = TaskKey::new(task);
    match signal {
        Signal::Working => {
            if let Some(phase) = signal_phase(hook) {
                if primary && phase == SpecPhase::Implement {
                    crate::handoff::on_code_edit(app, &key).await;
                }
                advance(app, &key, phase).await;
                // Only a push or a PR means review: GitHub knows of it in a moment.
                if phase == SpecPhase::Review {
                    crate::pr_loop::nudge(app, &key);
                }
            }
        }
        // Back to work: the question no longer stands.
        Signal::PromptSubmitted => drop_hint(app, task),
        Signal::TurnEnded if primary && !limited => {
            let app = app.clone();
            tokio::spawn(async move {
                crate::handoff::on_turn_ended(&app, &key).await;
                maybe_hint(&app, &key).await;
            });
        }
        Signal::TurnFailed if primary && !limited => {
            let app = app.clone();
            tokio::spawn(async move { crate::handoff::on_turn_failed(&app, &key).await });
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::store::Store;

    async fn running() -> (AppState, TaskKey) {
        let config = Config {
            home: std::env::temp_dir().join(format!("delilah-phases-{}", uuid::Uuid::new_v4())),
            addr: "127.0.0.1:7420".parse().unwrap(),
            allow_file_protocol: false,
            notify: false,
            ui_origin: None,
            agent_program: None,
            secrets_in_memory: false,
            jira_http_loopback: false,
            pr_poll: crate::config::DEFAULT_PR_POLL,
        };
        let app = AppState::new(Store::in_memory().await.unwrap(), config);
        let key = TaskKey::new("PROJ-1");
        app.store
            .create_task(&Task::new(key.clone(), "t", "feature"))
            .await
            .unwrap();
        app.transition(&key, Event::Start).await.unwrap();
        app.transition(&key, Event::WorkspaceReady).await.unwrap();
        (app, key)
    }

    async fn phase(app: &AppState, key: &TaskKey) -> Option<String> {
        app.store.get_task(key).await.unwrap().unwrap().phase
    }

    #[tokio::test]
    async fn signals_only_move_forward_but_the_user_moves_anywhere() {
        let (app, key) = running().await;
        advance(&app, &key, SpecPhase::Implement).await;
        assert_eq!(phase(&app, &key).await.as_deref(), Some("implement"));
        // A later file-based reading of "plan" does not take it back.
        advance(&app, &key, SpecPhase::Plan).await;
        assert_eq!(phase(&app, &key).await.as_deref(), Some("implement"));
        // The user can.
        assert!(record(&app, &key, SpecPhase::Plan).await.is_ok());
        assert_eq!(phase(&app, &key).await.as_deref(), Some("plan"));
    }

    #[test]
    fn edits_to_code_mean_implementing_and_a_pr_means_review() {
        use serde_json::json;
        let edit = |p: &str| json!({ "tool_name": "Edit", "tool_input": { "file_path": p } });
        assert_eq!(
            signal_phase(&edit("/w/ws/src/main.rs")),
            Some(SpecPhase::Implement)
        );
        assert_eq!(signal_phase(&edit("/w/ws/specs/001-x/spec.md")), None);
        let bash = |c: &str| json!({ "tool_name": "Bash", "tool_input": { "command": c } });
        assert_eq!(
            signal_phase(&bash("gh pr create --fill")),
            Some(SpecPhase::Review)
        );
        assert_eq!(
            signal_phase(&bash("git push -u origin feature/X")),
            Some(SpecPhase::Review)
        );
        assert_eq!(signal_phase(&bash("cargo test")), None);
    }

    #[tokio::test]
    async fn reaching_review_drops_the_hint() {
        let (app, key) = running().await;
        hint(&app, key.as_str());
        assert!(app.review_hints.lock().unwrap().contains("PROJ-1"));
        advance(&app, &key, SpecPhase::Review).await;
        assert!(!app.review_hints.lock().unwrap().contains("PROJ-1"));
    }

    /// A saving-mode task in its plan session, with a written spec in its workspace.
    async fn planning() -> (AppState, TaskKey) {
        let (app, key) = running().await;
        app.store.set_saving_mode(&key, true).await.unwrap();
        let path = app.config.home.join("ws");
        std::fs::create_dir_all(path.join("specs/proj-1-x")).unwrap();
        std::fs::write(path.join("specs/proj-1-x/tasks.md"), "- [ ] T001 One\n").unwrap();
        let ws = delilah_git::Workspace {
            path: path.clone(),
            branch: "feature/PROJ-1".into(),
            kind: delilah_core::ProjectKind::Repo,
            submodules: vec![],
        };
        app.store.save_workspace(&key, &path, &ws).await.unwrap();
        app.store
            .create_session(&crate::store_sessions::SessionRecord {
                id: "plan-1".into(),
                task: "PROJ-1".into(),
                kind: crate::store_sessions::SessionKind::Claude,
                label: "Spec y plan".into(),
                primary: true,
                closed: false,
                read_only: false,
                conversation: false,
                step: Some(delilah_core::Step::Plan),
            })
            .await
            .unwrap();
        (app, key)
    }

    async fn stage(app: &AppState, key: &TaskKey) -> Option<delilah_core::HandoffStage> {
        let h = app.store.get_task(key).await.unwrap().unwrap().handoff;
        h.map(|h| h.stage)
    }

    #[tokio::test]
    async fn a_primary_turn_end_checks_the_handoff_but_a_limited_one_does_not() {
        use delilah_agents::claude::Signal;
        let (app, key) = planning().await;
        let hook = serde_json::json!({});
        on_hook(&app, "PROJ-1", Signal::TurnEnded, &hook, true, true).await;
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        assert_eq!(stage(&app, &key).await, None);
        on_hook(&app, "PROJ-1", Signal::TurnEnded, &hook, true, false).await;
        for _ in 0..50 {
            if stage(&app, &key).await.is_some() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
        assert_eq!(
            stage(&app, &key).await,
            Some(delilah_core::HandoffStage::Offered)
        );
    }

    #[tokio::test]
    async fn a_limited_turn_end_or_failure_keeps_the_saving_stage() {
        use delilah_agents::claude::Signal;
        let (app, key) = planning().await;
        let saving = delilah_core::Handoff {
            to: delilah_core::Step::Implement,
            stage: delilah_core::HandoffStage::Saving,
            error: None,
        };
        app.store.set_handoff(&key, Some(&saving)).await.unwrap();
        let hook = serde_json::json!({});
        for signal in [Signal::TurnEnded, Signal::TurnFailed] {
            on_hook(&app, "PROJ-1", signal, &hook, true, true).await;
        }
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        assert_eq!(
            stage(&app, &key).await,
            Some(delilah_core::HandoffStage::Saving)
        );
    }

    #[tokio::test]
    async fn a_code_edit_from_the_plan_session_skips_the_handoff() {
        use delilah_agents::claude::Signal;
        let (app, key) = planning().await;
        let edit = serde_json::json!({ "tool_name": "Edit", "tool_input": { "file_path": "/w/ws/src/main.rs" } });
        on_hook(&app, "PROJ-1", Signal::Working, &edit, true, false).await;
        assert_eq!(
            stage(&app, &key).await,
            Some(delilah_core::HandoffStage::Skipped)
        );
    }

    #[tokio::test]
    async fn the_phase_route_offers_going_forward_to_implement_but_not_back() {
        let (app, key) = planning().await;
        let back = SetPhase {
            phase: "specify".into(),
        };
        let _ = set(State(app.clone()), Path("PROJ-1".into()), Json(back))
            .await
            .ok()
            .unwrap();
        assert_eq!(stage(&app, &key).await, None);
        let forward = SetPhase {
            phase: "implement".into(),
        };
        let _ = set(State(app.clone()), Path("PROJ-1".into()), Json(forward))
            .await
            .ok()
            .unwrap();
        assert_eq!(
            stage(&app, &key).await,
            Some(delilah_core::HandoffStage::Offered)
        );
    }

    #[tokio::test]
    async fn a_push_or_a_pr_nudges_the_pr_loop_and_other_commands_do_not() {
        use delilah_agents::claude::Signal;
        use serde_json::json;
        let fake = std::sync::Arc::new(crate::gh::fake::FakeGh::new());
        let (app, key) = running().await;
        let app = app.with_gh(fake.clone());
        app.store
            .save_workspace(
                &key,
                std::path::Path::new("/parent"),
                &delilah_git::Workspace {
                    path: "/ws".into(),
                    branch: "feature/PROJ-1".into(),
                    kind: delilah_core::ProjectKind::Repo,
                    submodules: vec![],
                },
            )
            .await
            .unwrap();
        let hook = |c: &str| json!({ "tool_name": "Bash", "tool_input": { "command": c } });
        for quiet in ["cargo test", "git status"] {
            on_hook(&app, "PROJ-1", Signal::Working, &hook(quiet), true, false).await;
        }
        assert!(app.pr_nudges.lock().unwrap().is_empty());
        assert!(fake.calls().is_empty());

        on_hook(
            &app,
            "PROJ-1",
            Signal::Working,
            &hook("git push -u origin x"),
            true,
            false,
        )
        .await;
        // The check itself waits for GitHub: what is asked of the loop is one pending nudge.
        assert!(app.pr_nudges.lock().unwrap().contains("PROJ-1"));
    }
}
