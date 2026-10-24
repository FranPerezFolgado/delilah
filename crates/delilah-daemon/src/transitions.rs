//! The one way the daemon changes a task's status, so every change is notified.

use chrono::{Local, Utc};
use delilah_core::{Event, Task, TaskKey, TaskStatus};

use crate::events::Topic;
use crate::state::AppState;
use crate::store::StoreError;

impl AppState {
    pub async fn transition(&self, key: &TaskKey, event: Event) -> Result<Task, StoreError> {
        self.change(key, event, true).await
    }

    /// Like [`AppState::transition`], without the notification of the change: for a
    /// caller that tells it in its own words (a merge that marks the task done).
    pub(crate) async fn transition_quiet(
        &self,
        key: &TaskKey,
        event: Event,
    ) -> Result<Task, StoreError> {
        self.change(key, event, false).await
    }

    /// A task is over (done, failed, archived, deleted): what runs for it ends. The
    /// place for per-task runtime cleanup; best effort, and a no-op when nothing runs.
    pub(crate) fn end_task_runtime(&self, key: &TaskKey) {
        self.previews.stop(key.as_str());
    }

    /// For the daemon's shutdown: nothing is left running behind it.
    pub async fn shutdown_runtime(&self) {
        self.previews.stop_all().await;
    }

    async fn change(&self, key: &TaskKey, event: Event, notify: bool) -> Result<Task, StoreError> {
        let before = self.store.get_task(key).await?.map(|t| t.status);
        let task = self.store.apply_event(key, event, Utc::now()).await?;
        if task.status.is_terminal() {
            self.end_task_runtime(key);
        }
        self.emit(Topic::Tasks);
        self.emit(Topic::Inbox);
        if notify
            && let Some(before) = before
            && before != task.status
        {
            self.notify_change(&task, &before);
        }
        Ok(task)
    }

    fn notify_change(&self, task: &Task, before: &TaskStatus) {
        // The app, when open, notifies itself (with the agent's question, and opening
        // it on click); the daemon covers the rest of the time.
        if self.events.listening() {
            return;
        }
        let key = &task.key;
        let l = self.lang();
        let (title, body) = match (&task.status, before) {
            (TaskStatus::WaitingForUser, _) => {
                // Whoever is attached is already looking at it.
                if self.sessions.watched(key.as_str()) {
                    return;
                }
                (
                    format!("{key} {}", l.pick("te espera", "is waiting for you")),
                    task.title.clone(),
                )
            }
            (TaskStatus::PausedByLimit { resume_at }, _) => (
                format!(
                    "{key} {}",
                    l.pick("pausada por límite", "paused by the limit")
                ),
                format!(
                    "{} {}",
                    l.pick("Se reanuda sola a las", "It resumes by itself at"),
                    resume_at.with_timezone(&Local).format("%H:%M")
                ),
            ),
            (TaskStatus::Running, TaskStatus::PausedByLimit { .. }) => (
                format!("{key} {}", l.pick("reanudada", "resumed")),
                task.title.clone(),
            ),
            (TaskStatus::Done, _) => (
                format!("{key} {}", l.pick("terminada", "done")),
                task.title.clone(),
            ),
            (TaskStatus::Failed { reason }, _) => (
                format!("{key} {}", l.pick("ha fallado", "failed")),
                reason.clone(),
            ),
            (TaskStatus::SetupFailed { reason }, _) => (
                format!("{key} {}", l.pick("el setup ha fallado", "setup failed")),
                match reason.as_str() {
                    "cancelled" => l.pick("cancelado", "cancelled").to_string(),
                    "interrupted" => l.pick("interrumpido", "interrupted").to_string(),
                    other => other.to_string(),
                },
            ),
            _ => return,
        };
        self.notifier.send(title, body);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::Store;

    async fn running_task_with_preview(key: &str) -> AppState {
        let config = crate::config::Config {
            home: std::env::temp_dir().join(format!("delilah-tr-{}", uuid::Uuid::new_v4())),
            addr: crate::config::DEFAULT_ADDR.parse().unwrap(),
            allow_file_protocol: false,
            notify: false,
            ui_origin: None,
            agent_program: None,
            claude_bin: "claude".into(),
            ..crate::config::Config::test_defaults()
        };
        let app = AppState::new(Store::in_memory().await.unwrap(), config);
        let key = TaskKey::new(key);
        app.store
            .create_task(&Task::new(key.clone(), "x", "feature"))
            .await
            .unwrap();
        for event in [Event::Start, Event::WorkspaceReady] {
            app.store
                .apply_event(&key, event, Utc::now())
                .await
                .unwrap();
        }
        app.previews
            .start(
                key.as_str(),
                &std::env::temp_dir(),
                "echo http://127.0.0.1:9/x; sleep 30",
            )
            .unwrap();
        app
    }

    #[tokio::test]
    async fn done_stops_the_preview() {
        let app = running_task_with_preview("T-1").await;
        app.transition(&TaskKey::new("T-1"), Event::MarkDone)
            .await
            .unwrap();
        assert!(app.previews.view("T-1").is_none());
    }

    #[tokio::test]
    async fn failing_stops_the_preview() {
        let app = running_task_with_preview("T-1").await;
        app.transition(&TaskKey::new("T-1"), Event::Fail { reason: "x".into() })
            .await
            .unwrap();
        assert!(app.previews.view("T-1").is_none());
    }

    #[tokio::test]
    async fn a_quiet_done_stops_the_preview_too() {
        let app = running_task_with_preview("T-1").await;
        app.transition_quiet(&TaskKey::new("T-1"), Event::MarkDone)
            .await
            .unwrap();
        assert!(app.previews.view("T-1").is_none());
    }

    #[tokio::test]
    async fn a_non_terminal_change_keeps_the_preview() {
        let app = running_task_with_preview("T-1").await;
        let task = app
            .transition(&TaskKey::new("T-1"), Event::AgentAsked)
            .await
            .unwrap();
        assert!(!task.status.is_terminal());
        assert!(app.previews.view("T-1").is_some());
        app.previews.stop("T-1");
    }

    #[tokio::test]
    async fn stop_all_ends_every_group() {
        let app = running_task_with_preview("T-1").await;
        let dir = std::env::temp_dir();
        let pid_file = dir.join(format!("delilah-stopall-{}", uuid::Uuid::new_v4()));
        app.previews
            .start(
                "T-2",
                &dir,
                &format!("sleep 30 & echo $! > {}; wait", pid_file.display()),
            )
            .unwrap();
        let mut pid = String::new();
        for _ in 0..200 {
            pid = std::fs::read_to_string(&pid_file).unwrap_or_default();
            if pid.ends_with('\n') {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(25)).await;
        }
        let _ = std::fs::remove_file(&pid_file);
        let pid = pid.trim().to_string();
        let alive = || {
            std::process::Command::new("/bin/kill")
                .args(["-0", &pid])
                .stderr(std::process::Stdio::null())
                .status()
                .unwrap()
                .success()
        };
        assert!(alive());
        app.shutdown_runtime().await;
        assert!(app.previews.view("T-1").is_none());
        assert!(app.previews.view("T-2").is_none());
        for _ in 0..100 {
            if !alive() {
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(25)).await;
        }
        panic!("a group is still alive");
    }
}
