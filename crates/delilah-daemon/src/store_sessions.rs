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
    /// A project's setup command running in a new worktree: read-only, no input
    /// reaches it, and it closes when the command ends. Its log is the full output.
    Setup,
}

impl SessionKind {
    pub fn name(self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Shell => "shell",
            Self::Setup => "setup",
        }
    }

    fn parse(s: &str) -> Option<Self> {
        match s {
            "claude" => Some(Self::Claude),
            "shell" => Some(Self::Shell),
            "setup" => Some(Self::Setup),
            _ => None,
        }
    }
}

/// Sessions of a project (not of a task) are owned by `@<project>`: the key every
/// other part of the daemon (hooks, inbox, files) uses for them.
pub const PROJECT_OWNER: char = '@';

pub fn project_of(owner: &str) -> Option<&str> {
    owner.strip_prefix(PROJECT_OWNER)
}

pub fn project_owner(project: &str) -> String {
    format!("{PROJECT_OWNER}{project}")
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SessionRecord {
    pub id: String,
    /// The task's key, or `@<project>` for a project's session.
    pub task: String,
    pub kind: SessionKind,
    pub label: String,
    pub primary: bool,
    pub closed: bool,
    /// Claude in plan mode: it reads and explains, but does not edit.
    pub read_only: bool,
}

impl Store {
    pub async fn create_session(&self, s: &SessionRecord) -> Result<(), StoreError> {
        sqlx::query(
            "INSERT INTO sessions (id, task_key, project, kind, label, is_primary, closed, created_at, read_only)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&s.id)
        .bind(project_of(&s.task).is_none().then_some(&s.task))
        .bind(project_of(&s.task))
        .bind(s.kind.name())
        .bind(&s.label)
        .bind(s.primary)
        .bind(s.closed)
        .bind(Utc::now().to_rfc3339())
        .bind(s.read_only)
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
            Some(owner) => {
                let column = if project_of(owner).is_some() {
                    "project"
                } else {
                    "task_key"
                };
                sqlx::query(&format!(
                    "{SELECT} WHERE closed = 0 AND {column} = ? ORDER BY created_at, id"
                ))
                .bind(project_of(owner).unwrap_or(owner))
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

    /// Moves a project's session to a task, as its primary, able to edit.
    pub async fn adopt_session(&self, id: &str, task: &str, label: &str) -> Result<(), StoreError> {
        sqlx::query(
            "UPDATE sessions SET task_key = ?, project = NULL, is_primary = 1, read_only = 0,
             label = ? WHERE id = ?",
        )
        .bind(task)
        .bind(label)
        .bind(id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn set_read_only(&self, id: &str, read_only: bool) -> Result<(), StoreError> {
        sqlx::query("UPDATE sessions SET read_only = ? WHERE id = ?")
            .bind(read_only)
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn close_session(&self, id: &str) -> Result<(), StoreError> {
        sqlx::query("UPDATE sessions SET closed = 1 WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}

const SELECT: &str =
    "SELECT id, task_key, project, kind, label, is_primary, closed, read_only FROM sessions";

impl Store {
    /// Records what a session waits on the user for, or clears it.
    pub async fn set_attention(
        &self,
        session: &str,
        attention: Option<&crate::inbox::Attention>,
    ) -> Result<(), StoreError> {
        let json = attention.map(serde_json::to_string).transpose()?;
        sqlx::query("UPDATE sessions SET attention = ? WHERE id = ?")
            .bind(json)
            .bind(session)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn attention(
        &self,
        session: &str,
    ) -> Result<Option<crate::inbox::Attention>, StoreError> {
        let json: Option<String> =
            sqlx::query_scalar("SELECT attention FROM sessions WHERE id = ?")
                .bind(session)
                .fetch_optional(&self.pool)
                .await?
                .flatten();
        Ok(json.map(|j| serde_json::from_str(&j)).transpose()?)
    }

    /// Open sessions that wait on the user, with what for.
    pub async fn attentions(
        &self,
    ) -> Result<Vec<(SessionRecord, crate::inbox::Attention)>, StoreError> {
        let rows = sqlx::query(
            "SELECT id, task_key, project, kind, label, is_primary, closed, read_only, attention FROM sessions
             WHERE closed = 0 AND attention IS NOT NULL ORDER BY created_at, id",
        )
        .fetch_all(&self.pool)
        .await?;
        rows.iter()
            .map(|row| {
                let json: String = row.try_get("attention")?;
                Ok((session_from_row(row)?, serde_json::from_str(&json)?))
            })
            .collect()
    }
}

fn session_from_row(row: &SqliteRow) -> Result<SessionRecord, StoreError> {
    let kind: String = row.try_get("kind")?;
    Ok(SessionRecord {
        id: row.try_get("id")?,
        task: match row.try_get::<Option<String>, _>("task_key")? {
            Some(task) => task,
            None => project_owner(&row.try_get::<String, _>("project")?),
        },
        kind: SessionKind::parse(&kind)
            .ok_or_else(|| StoreError::Corrupt(format!("unknown session kind `{kind}`")))?,
        label: row.try_get("label")?,
        primary: row.try_get("is_primary")?,
        closed: row.try_get("closed")?,
        read_only: row.try_get("read_only")?,
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
            read_only: false,
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
