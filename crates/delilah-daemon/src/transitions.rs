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

    async fn change(&self, key: &TaskKey, event: Event, notify: bool) -> Result<Task, StoreError> {
        let before = self.store.get_task(key).await?.map(|t| t.status);
        let task = self.store.apply_event(key, event, Utc::now()).await?;
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
            _ => return,
        };
        self.notifier.send(title, body);
    }
}
