//! The sessions opened in each task's workspace.

use chrono::Utc;
use serde::{Deserialize, Serialize};
use sqlx::Row;
use sqlx::sqlite::SqliteRow;

use crate::store::{Store, StoreError};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SessionKind {
    Claude,
    /// The user's login shell in the workspace folder.
    Shell,
}

impl SessionKind {
    pub fn name(self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Shell => "shell",
        }
    }

    fn parse(s: &str) -> Option<Self> {
        match s {
            "claude" => Some(Self::Claude),
            "shell" => Some(Self::Shell),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SessionRecord {
    pub id: String,
    pub task: String,
    pub kind: SessionKind,
    pub label: String,
    pub primary: bool,
    pub closed: bool,
}

impl Store {
    pub async fn create_session(&self, s: &SessionRecord) -> Result<(), StoreError> {
        sqlx::query(
            "INSERT INTO sessions (id, task_key, kind, label, is_primary, closed, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&s.id)
        .bind(&s.task)
        .bind(s.kind.name())
        .bind(&s.label)
        .bind(s.primary)
        .bind(s.closed)
        .bind(Utc::now().to_rfc3339())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn session(&self, id: &str) -> Result<Option<SessionRecord>, StoreError> {
        let row = sqlx::query(&format!("{SELECT} WHERE id = ?"))
            .bind(id)
            .fetch_optional(&self.pool)
            .await?;
        row.as_ref().map(session_from_row).transpose()
    }

    /// Open sessions of one task, or of all tasks, oldest first.
    pub async fn open_sessions(
        &self,
        task: Option<&str>,
    ) -> Result<Vec<SessionRecord>, StoreError> {
        let rows = match task {
            Some(task) => {
                sqlx::query(&format!(
                    "{SELECT} WHERE closed = 0 AND task_key = ? ORDER BY created_at, id"
                ))
                .bind(task)
                .fetch_all(&self.pool)
                .await?
            }
            None => {
                sqlx::query(&format!(
                    "{SELECT} WHERE closed = 0 ORDER BY created_at, id"
                ))
                .fetch_all(&self.pool)
                .await?
            }
        };
        rows.iter().map(session_from_row).collect()
    }

    pub async fn primary_session(&self, task: &str) -> Result<Option<SessionRecord>, StoreError> {
        let row = sqlx::query(&format!("{SELECT} WHERE task_key = ? AND is_primary = 1"))
            .bind(task)
            .fetch_optional(&self.pool)
            .await?;
        row.as_ref().map(session_from_row).transpose()
    }

    pub async fn close_session(&self, id: &str) -> Result<(), StoreError> {
        sqlx::query("UPDATE sessions SET closed = 1 WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}

const SELECT: &str = "SELECT id, task_key, kind, label, is_primary, closed FROM sessions";

fn session_from_row(row: &SqliteRow) -> Result<SessionRecord, StoreError> {
    let kind: String = row.try_get("kind")?;
    Ok(SessionRecord {
        id: row.try_get("id")?,
        task: row.try_get("task_key")?,
        kind: SessionKind::parse(&kind)
            .ok_or_else(|| StoreError::Corrupt(format!("unknown session kind `{kind}`")))?,
        label: row.try_get("label")?,
        primary: row.try_get("is_primary")?,
        closed: row.try_get("closed")?,
    })
}

#[cfg(test)]
mod tests {
    use delilah_core::{Task, TaskKey};

    use super::*;

    fn session(id: &str, kind: SessionKind, primary: bool) -> SessionRecord {
        SessionRecord {
            id: id.into(),
            task: "PROJ-1".into(),
            kind,
            label: id.into(),
            primary,
            closed: false,
        }
    }

    #[tokio::test]
    async fn keeps_a_tasks_sessions_and_its_primary() {
        let store = Store::in_memory().await.unwrap();
        store
            .create_task(&Task::new(TaskKey::new("PROJ-1"), "t", "feature"))
            .await
            .unwrap();
        store
            .create_session(&session("a", SessionKind::Claude, true))
            .await
            .unwrap();
        store
            .create_session(&session("b", SessionKind::Claude, false))
            .await
            .unwrap();
        store
            .create_session(&session("c", SessionKind::Shell, false))
            .await
            .unwrap();

        assert_eq!(
            store.primary_session("PROJ-1").await.unwrap().unwrap().id,
            "a"
        );
        assert_eq!(store.open_sessions(Some("PROJ-1")).await.unwrap().len(), 3);

        store.close_session("b").await.unwrap();
        let open: Vec<_> = store
            .open_sessions(None)
            .await
            .unwrap()
            .into_iter()
            .map(|s| s.id)
            .collect();
        assert_eq!(open, ["a", "c"]);
        assert!(store.session("b").await.unwrap().unwrap().closed);
        assert_eq!(
            store.session("c").await.unwrap().unwrap().kind,
            SessionKind::Shell
        );
    }
}
