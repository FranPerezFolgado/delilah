//! The one way the daemon changes a task's status, so every change is notified.

use chrono::{Local, Utc};
use delilah_core::{Event, Task, TaskKey, TaskStatus};

use crate::events::Topic;
use crate::state::AppState;
use crate::store::StoreError;

impl AppState {
    pub async fn transition(&self, key: &TaskKey, event: Event) -> Result<Task, StoreError> {
        let before = self.store.get_task(key).await?.map(|t| t.status);
        let task = self.store.apply_event(key, event, Utc::now()).await?;
        self.emit(Topic::Tasks);
        self.emit(Topic::Inbox);
        if let Some(before) = before
            && before != task.status
        {
            self.notify_change(&task, &before);
        }
        Ok(task)
    }

    fn notify_change(&self, task: &Task, before: &TaskStatus) {
        let key = &task.key;
        let (title, body) = match (&task.status, before) {
            (TaskStatus::WaitingForUser, _) => {
                // Whoever is attached is already looking at it.
                if self.sessions.watched(key.as_str()) {
                    return;
                }
                (format!("{key} te espera"), task.title.clone())
            }
            (TaskStatus::PausedByLimit { resume_at }, _) => (
                format!("{key} pausada por límite"),
                format!(
                    "Se reanuda sola a las {}",
                    resume_at.with_timezone(&Local).format("%H:%M")
                ),
            ),
            (TaskStatus::Running, TaskStatus::PausedByLimit { .. }) => {
                (format!("{key} reanudada"), task.title.clone())
            }
            (TaskStatus::Done, _) => (format!("{key} terminada"), task.title.clone()),
            (TaskStatus::Failed { reason }, _) => (format!("{key} ha fallado"), reason.clone()),
            _ => return,
        };
        self.notifier.send(title, body);
    }
}
