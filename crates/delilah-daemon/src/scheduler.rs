//! Periodic work: resumes tasks paused by a usage limit once the limit resets.

use std::time::Duration;

use crate::actions::resume_if_due;
use crate::state::AppState;

pub const TICK: Duration = Duration::from_secs(30);

pub fn spawn(app: AppState) {
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(TICK);
        loop {
            ticker.tick().await;
            tick(&app).await;
        }
    });
}

pub async fn tick(app: &AppState) {
    let tasks = match app.store.list_tasks().await {
        Ok(tasks) => tasks,
        Err(e) => return tracing::warn!("scheduler: {e}"),
    };
    for task in tasks {
        resume_if_due(app, &task).await;
        crate::handoff::recover(app, &task).await;
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use chrono::Utc;
    use delilah_agents::pty::PtySession;
    use delilah_core::{Event, Task, TaskKey, TaskStatus};
    use portable_pty::CommandBuilder;

    use super::*;
    use crate::actions::continue_prompt;
    use crate::config::Config;
    use crate::store::Store;
    use crate::store_sessions::SessionKind;

    async fn paused_until(
        resume_at: chrono::DateTime<Utc>,
    ) -> (AppState, std::sync::Arc<PtySession>) {
        let config = Config {
            home: std::env::temp_dir().join(format!("delilah-sched-{}", uuid::Uuid::new_v4())),
            addr: "127.0.0.1:7420".parse().unwrap(),
            allow_file_protocol: false,
            notify: false,
            ui_origin: None,
            agent_program: None,
        };
        let app = AppState::new(Store::in_memory().await.unwrap(), config);
        let key = TaskKey::new("PROJ-1");
        app.store
            .create_task(&Task::new(key.clone(), "Card retries", "feature"))
            .await
            .unwrap();
        for event in [
            Event::Start,
            Event::WorkspaceReady,
            Event::LimitReached { resume_at },
        ] {
            app.transition(&key, event).await.unwrap();
        }
        // Stands in for the paused agent: echoes the next line it is sent.
        let mut cmd = CommandBuilder::new("bash");
        cmd.args([
            "--noprofile",
            "--norc",
            "-c",
            "read line; echo \"got:$line\"; sleep 5",
        ]);
        let pty = PtySession::spawn(cmd, 200, 24, None).unwrap();
        app.sessions
            .insert("s-1", "PROJ-1", SessionKind::Claude, true, pty.clone());
        (app, pty)
    }

    async fn status(app: &AppState) -> TaskStatus {
        app.store
            .get_task(&TaskKey::new("PROJ-1"))
            .await
            .unwrap()
            .unwrap()
            .status
    }

    #[tokio::test]
    async fn resumes_a_paused_task_once_its_reset_has_passed() {
        let (app, pty) = paused_until(Utc::now() - chrono::Duration::seconds(1)).await;
        let (_, mut output) = pty.attach();

        tick(&app).await;
        assert_eq!(status(&app).await, TaskStatus::Running);

        let mut seen = String::new();
        tokio::time::timeout(Duration::from_secs(5), async {
            while !seen.contains("got:Se ha restablecido") {
                seen.push_str(&String::from_utf8_lossy(&output.recv().await.unwrap()));
            }
        })
        .await
        .unwrap_or_else(|_| panic!("the agent never got the prompt: {seen:?}"));
        assert!(continue_prompt(crate::lang::Lang::Es).starts_with("Se ha restablecido"));
        assert_eq!(
            app.notifier.recorded().last().unwrap().title,
            "PROJ-1 reanudada"
        );
    }

    #[tokio::test]
    async fn leaves_alone_a_task_whose_reset_is_still_ahead() {
        let (app, _pty) = paused_until(Utc::now() + chrono::Duration::hours(1)).await;
        tick(&app).await;
        assert_eq!(status(&app).await.name(), "paused-by-limit");
    }

    #[tokio::test]
    async fn recovery_on_tick_clears_a_handoff_whose_step_session_exists() {
        use delilah_core::{Handoff, HandoffStage, Step};
        let (app, _pty) = paused_until(Utc::now() + chrono::Duration::hours(1)).await;
        let key = TaskKey::new("PROJ-1");
        app.store.set_saving_mode(&key, true).await.unwrap();
        app.store
            .create_session(&crate::store_sessions::SessionRecord {
                id: "impl-1".into(),
                task: "PROJ-1".into(),
                kind: SessionKind::Claude,
                label: "Implementación".into(),
                primary: false,
                closed: false,
                read_only: false,
                conversation: false,
                step: Some(Step::Implement),
            })
            .await
            .unwrap();
        // That session is running, so it was launched: only the handoff is left.
        app.sessions
            .insert("impl-1", "PROJ-1", SessionKind::Claude, false, _pty.clone());
        let h = |stage| Handoff {
            to: Step::Implement,
            stage,
            error: None,
        };
        app.store
            .set_handoff(&key, Some(&h(HandoffStage::Starting)))
            .await
            .unwrap();
        tick(&app).await;
        let task = app.store.get_task(&key).await.unwrap().unwrap();
        assert!(task.handoff.is_none());

        // A save with no outgoing session running goes back to the offer.
        app.store
            .set_handoff(&key, Some(&h(HandoffStage::Saving)))
            .await
            .unwrap();
        tick(&app).await;
        let task = app.store.get_task(&key).await.unwrap().unwrap();
        assert_eq!(task.handoff.unwrap().stage, HandoffStage::Offered);
    }

    #[tokio::test]
    async fn recovery_on_tick_reopens_a_pr_of_an_autopilot_left_opening() {
        use std::sync::Arc;

        use crate::gh::fake::{FakeGh, PR_LIST, PR_URL, REPO_VIEW, bare_snapshot};
        use crate::pr_testkit::{git_repo_with_origin, task_with_workspace};
        use delilah_core::{Autopilot, AutopilotStage, AutopilotState};

        let gh = Arc::new(
            FakeGh::new()
                .with_user("me")
                .on(&["repo", "view"], Ok(REPO_VIEW))
                .on(&["pr", "list"], Ok(PR_LIST))
                .on(&["pr", "create"], Ok(PR_URL))
                .on(&["api", "graphql"], Ok(&bare_snapshot("OPEN", false))),
        );
        let (app, key, dir) = task_with_workspace(gh.clone(), None).await;
        git_repo_with_origin(
            &dir.path().join("ws"),
            &dir.path().join("origin"),
            "feature/PROJ-1",
            &["first"],
        );
        app.store
            .create_session(&crate::store_sessions::SessionRecord {
                id: "rev-1".into(),
                task: "PROJ-1".into(),
                kind: SessionKind::Claude,
                label: "Revisión".into(),
                primary: true,
                closed: false,
                read_only: false,
                conversation: false,
                step: Some(delilah_core::Step::Review),
            })
            .await
            .unwrap();
        // A second task with no autopilot is left alone.
        app.store
            .create_task(&Task::new(TaskKey::new("PROJ-2"), "other", "feature"))
            .await
            .unwrap();
        let on = |stage| Autopilot {
            state: AutopilotState::Running,
            stage: Some(stage),
            reason: None,
            since: Utc::now(),
            nudged: None,
        };
        // Preview stage with a lost process and no preview command: on to the PR.
        app.store
            .set_autopilot(&key, Some(&on(AutopilotStage::Preview)))
            .await
            .unwrap();
        tick(&app).await;
        let task = app.store.get_task(&key).await.unwrap().unwrap();
        assert_eq!(task.autopilot.unwrap().state, AutopilotState::Done);
        assert_eq!(app.store.prs_of(&key).await.unwrap().len(), 1);
        assert!(
            !gh.calls()
                .iter()
                .any(|c| c.starts_with(&["pr".to_string(), "create".to_string()]))
        );
        let other = app
            .store
            .get_task(&TaskKey::new("PROJ-2"))
            .await
            .unwrap()
            .unwrap();
        assert!(other.autopilot.is_none());
        assert!(
            app.store
                .prs_of(&TaskKey::new("PROJ-2"))
                .await
                .unwrap()
                .is_empty()
        );
    }
}
