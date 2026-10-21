//! The sessions opened in each task's workspace.

use chrono::Utc;
use delilah_core::{Event, Step};
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
    /// The step of the saving-mode chain it runs; `None` for any other session.
    pub step: Option<Step>,
}

impl Store {
    pub async fn create_session(&self, s: &SessionRecord) -> Result<(), StoreError> {
        insert_session(&self.pool, s).await
    }

    /// The session that runs `step` of a task's saving-mode chain, if there is one.
    pub async fn session_of_step(
        &self,
        task: &str,
        step: Step,
    ) -> Result<Option<SessionRecord>, StoreError> {
        let row = sqlx::query(&format!("{SELECT} WHERE task_key = ? AND step = ?"))
            .bind(task)
            .bind(step.name())
            .fetch_optional(&self.pool)
            .await?;
        row.as_ref().map(session_from_row).transpose()
    }

    /// Makes `new` (a record with its `step`) the task's only primary session and
    /// records `StepStarted`, in one transaction. If the step already has a session
    /// (a retry), that one is returned and nothing changes. The previous session is
    /// left listed; stopping it is up to the caller.
    pub async fn hand_over(
        &self,
        task: &str,
        from_id: &str,
        new: &SessionRecord,
    ) -> Result<SessionRecord, StoreError> {
        let step = new
            .step
            .ok_or_else(|| StoreError::Corrupt("a handoff session needs a step".into()))?;
        // IMMEDIATE, as in `apply_event`: a retry waits for the first attempt's result.
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let existing = sqlx::query(&format!("{SELECT} WHERE task_key = ? AND step = ?"))
            .bind(task)
            .bind(step.name())
            .fetch_optional(&mut *tx)
            .await?;
        if let Some(row) = existing {
            return session_from_row(&row);
        }
        sqlx::query("UPDATE sessions SET is_primary = 0 WHERE task_key = ? AND id = ?")
            .bind(task)
            .bind(from_id)
            .execute(&mut *tx)
            .await?;
        let record = SessionRecord {
            primary: true,
            closed: false,
            ..new.clone()
        };
        insert_session(&mut *tx, &record).await?;
        sqlx::query("INSERT INTO task_events (task_key, at, event) VALUES (?, ?, ?)")
            .bind(task)
            .bind(Utc::now().to_rfc3339())
            .bind(serde_json::to_string(&Event::StepStarted {
                step: step.name().into(),
                session: record.id.clone(),
            })?)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(record)
    }

    /// Reverts `hand_over`: `from_id` is the primary again and the new session is
    /// closed (kept, not deleted, so the history still points at it).
    pub async fn undo_hand_over(
        &self,
        task: &str,
        from_id: &str,
        new_id: &str,
    ) -> Result<(), StoreError> {
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        sqlx::query("UPDATE sessions SET is_primary = 0, closed = 1 WHERE task_key = ? AND id = ?")
            .bind(task)
            .bind(new_id)
            .execute(&mut *tx)
            .await?;
        sqlx::query("UPDATE sessions SET is_primary = 1 WHERE task_key = ? AND id = ?")
            .bind(task)
            .bind(from_id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
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

async fn insert_session<'e>(
    executor: impl sqlx::SqliteExecutor<'e>,
    s: &SessionRecord,
) -> Result<(), StoreError> {
    sqlx::query(
        "INSERT INTO sessions (id, task_key, project, kind, label, is_primary, closed, created_at, read_only, step)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
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
    .bind(s.step.map(Step::name))
    .execute(executor)
    .await?;
    Ok(())
}

const SELECT: &str =
    "SELECT id, task_key, project, kind, label, is_primary, closed, read_only, step FROM sessions";

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
            "SELECT id, task_key, project, kind, label, is_primary, closed, read_only, step, attention FROM sessions
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
        step: row
            .try_get::<Option<String>, _>("step")?
            .map(|s| {
                Step::parse(&s)
                    .ok_or_else(|| StoreError::Corrupt(format!("unknown session step `{s}`")))
            })
            .transpose()?,
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
            step: None,
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

    fn step_session(id: &str, step: Step) -> SessionRecord {
        SessionRecord {
            step: Some(step),
            ..session(id, SessionKind::Claude, false)
        }
    }

    async fn store_with_plan_session() -> Store {
        let store = Store::in_memory().await.unwrap();
        store
            .create_task(&Task::new(TaskKey::new("PROJ-1"), "t", "feature"))
            .await
            .unwrap();
        let plan = SessionRecord {
            primary: true,
            ..step_session("plan", Step::Plan)
        };
        store.create_session(&plan).await.unwrap();
        store
    }

    #[tokio::test]
    async fn a_session_remembers_its_step() {
        let store = store_with_plan_session().await;
        let found = store.session_of_step("PROJ-1", Step::Plan).await.unwrap();
        assert_eq!(found.unwrap().id, "plan");
        assert_eq!(
            store.session("plan").await.unwrap().unwrap().step,
            Some(Step::Plan)
        );
        assert!(
            store
                .session_of_step("PROJ-1", Step::Review)
                .await
                .unwrap()
                .is_none()
        );
    }

    #[tokio::test]
    async fn a_handover_leaves_one_primary_and_is_safe_to_retry() {
        let store = store_with_plan_session().await;
        let new = step_session("impl", Step::Implement);

        let made = store.hand_over("PROJ-1", "plan", &new).await.unwrap();
        assert!(made.primary);
        assert_eq!(
            store.primary_session("PROJ-1").await.unwrap().unwrap().id,
            "impl"
        );
        let primaries: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM sessions WHERE task_key = 'PROJ-1' AND is_primary = 1",
        )
        .fetch_one(&store.pool)
        .await
        .unwrap();
        assert_eq!(primaries, 1);
        assert!(!store.session("plan").await.unwrap().unwrap().primary);

        let history = store.events(&TaskKey::new("PROJ-1")).await.unwrap();
        assert_eq!(
            history.last().unwrap().event,
            Event::StepStarted {
                step: "implement".into(),
                session: "impl".into()
            }
        );

        // Again for the same step: the existing session, nothing new.
        let again = step_session("impl-2", Step::Implement);
        let same = store.hand_over("PROJ-1", "plan", &again).await.unwrap();
        assert_eq!(same.id, "impl");
        assert!(store.session("impl-2").await.unwrap().is_none());
        assert_eq!(
            store.events(&TaskKey::new("PROJ-1")).await.unwrap().len(),
            1
        );
    }

    #[tokio::test]
    async fn undoing_a_handover_restores_the_old_primary() {
        let store = store_with_plan_session().await;
        store
            .hand_over("PROJ-1", "plan", &step_session("impl", Step::Implement))
            .await
            .unwrap();

        store
            .undo_hand_over("PROJ-1", "plan", "impl")
            .await
            .unwrap();
        assert_eq!(
            store.primary_session("PROJ-1").await.unwrap().unwrap().id,
            "plan"
        );
        let undone = store.session("impl").await.unwrap().unwrap();
        assert!(undone.closed && !undone.primary);
    }
}
