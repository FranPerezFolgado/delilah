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
        crate::autopilot::recover(app, &task);
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
            claude_bin: "claude".into(),
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

    /// A task on autopilot with a review session, a repo with one commit, and a `gh`
    /// that lists `list` for `pr list` and sleeps `delay` on every call.
    async fn finishing_app(
        list: &str,
        delay: Duration,
    ) -> (
        AppState,
        TaskKey,
        std::sync::Arc<crate::gh::fake::FakeGh>,
        crate::pr_testkit::TempDir,
    ) {
        use std::sync::Arc;

        use crate::gh::fake::{FakeGh, PR_URL, REPO_VIEW, bare_snapshot};
        use crate::pr_testkit::{git_repo_with_origin, task_with_workspace};

        struct Slow(Arc<FakeGh>, Duration);
        impl crate::gh::Gh for Slow {
            fn run(
                &self,
                dir: Option<&std::path::Path>,
                args: Vec<String>,
                stdin: Option<String>,
            ) -> futures_util::future::BoxFuture<'_, Result<String, crate::gh::GhError>>
            {
                let delay = self.1;
                let inner = self.0.run(dir, args, stdin);
                Box::pin(async move {
                    tokio::time::sleep(delay).await;
                    inner.await
                })
            }
        }
        let fake = Arc::new(
            FakeGh::new()
                .with_user("me")
                .on(&["repo", "view"], Ok(REPO_VIEW))
                .on(&["pr", "list"], Ok(list))
                .on(&["pr", "create"], Ok(PR_URL))
                .on(&["api", "graphql"], Ok(&bare_snapshot("OPEN", false))),
        );
        let (app, key, dir) = task_with_workspace(fake.clone(), None).await;
        let app = app.with_gh(Arc::new(Slow(fake.clone(), delay)));
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
        (app, key, fake, dir)
    }

    fn on_stage(stage: delilah_core::AutopilotStage) -> delilah_core::Autopilot {
        delilah_core::Autopilot {
            state: delilah_core::AutopilotState::Running,
            stage: Some(stage),
            reason: None,
            since: Utc::now(),
            nudged: None,
            review_session: Some("rev-1".into()),
        }
    }

    async fn autopilot_state(app: &AppState, key: &TaskKey) -> delilah_core::AutopilotState {
        app.store
            .get_task(key)
            .await
            .unwrap()
            .unwrap()
            .autopilot
            .unwrap()
            .state
    }

    async fn until_done(app: &AppState, key: &TaskKey) {
        for _ in 0..200 {
            if autopilot_state(app, key).await == delilah_core::AutopilotState::Done {
                return;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        panic!("the autopilot never finished");
    }

    fn creates(gh: &crate::gh::fake::FakeGh) -> usize {
        gh.calls()
            .iter()
            .filter(|c| c.starts_with(&["pr".to_string(), "create".to_string()]))
            .count()
    }

    #[tokio::test]
    async fn recovery_on_tick_reopens_a_pr_of_an_autopilot_left_opening() {
        use crate::gh::fake::PR_LIST;
        use delilah_core::AutopilotStage;

        let (app, key, gh, _dir) = finishing_app(PR_LIST, Duration::ZERO).await;
        // A second task with no autopilot is left alone.
        app.store
            .create_task(&Task::new(TaskKey::new("PROJ-2"), "other", "feature"))
            .await
            .unwrap();
        // Preview stage with a lost process: on to the PR, linking the one gh has.
        app.store
            .set_autopilot(&key, Some(&on_stage(AutopilotStage::Preview)))
            .await
            .unwrap();
        tick(&app).await;
        until_done(&app, &key).await;
        assert_eq!(app.store.prs_of(&key).await.unwrap().len(), 1);
        assert_eq!(creates(&gh), 0);
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

    #[tokio::test]
    async fn recovery_on_tick_opens_the_pr_of_an_autopilot_at_opening_pr_once() {
        use delilah_core::AutopilotStage;

        let (app, key, gh, _dir) = finishing_app("[]", Duration::ZERO).await;
        app.store
            .set_autopilot(&key, Some(&on_stage(AutopilotStage::OpeningPr)))
            .await
            .unwrap();
        tick(&app).await;
        tick(&app).await;
        until_done(&app, &key).await;
        assert_eq!(creates(&gh), 1);
        // Done: the next ticks leave it alone.
        tick(&app).await;
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert_eq!(creates(&gh), 1);
    }

    #[tokio::test]
    async fn a_slow_finish_does_not_hold_up_the_tick() {
        use crate::gh::fake::PR_LIST;
        use delilah_core::AutopilotStage;

        let (app, key, _gh, _dir) = finishing_app(PR_LIST, Duration::from_millis(400)).await;
        app.store
            .set_autopilot(&key, Some(&on_stage(AutopilotStage::OpeningPr)))
            .await
            .unwrap();
        let started = std::time::Instant::now();
        tick(&app).await;
        assert!(
            started.elapsed() < Duration::from_millis(300),
            "the tick waited for the finish: {:?}",
            started.elapsed()
        );
        assert_eq!(
            autopilot_state(&app, &key).await,
            delilah_core::AutopilotState::Running
        );
        until_done(&app, &key).await;
    }
}
