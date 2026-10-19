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
    use crate::actions::CONTINUE_PROMPT;
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
        assert!(CONTINUE_PROMPT.starts_with("Se ha restablecido"));
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
}
