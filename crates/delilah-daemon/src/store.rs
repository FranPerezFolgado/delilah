use std::path::Path;
use std::str::FromStr;

use chrono::{DateTime, Utc};
use delilah_core::{Event, Task, TaskKey, TaskStatus, TransitionError};
use serde::Serialize;
use sqlx::Row;
use sqlx::sqlite::{
    SqliteConnectOptions, SqliteJournalMode, SqlitePool, SqlitePoolOptions, SqliteRow,
};

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("task {0} does not exist")]
    NotFound(TaskKey),
    #[error("task {0} already exists")]
    AlreadyExists(TaskKey),
    #[error(transparent)]
    Transition(#[from] TransitionError),
    #[error("database error: {0}")]
    Db(#[from] sqlx::Error),
    #[error("migration error: {0}")]
    Migrate(#[from] sqlx::migrate::MigrateError),
    #[error("corrupt stored value: {0}")]
    Json(#[from] serde_json::Error),
}

/// One applied event, as kept in a task's history.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EventRecord {
    pub at: DateTime<Utc>,
    pub event: Event,
}

/// Durable state of the daemon, so a restart (or a reboot) loses nothing.
#[derive(Debug, Clone)]
pub struct Store {
    pool: SqlitePool,
}

impl Store {
    pub async fn open(path: &Path) -> Result<Self, StoreError> {
        let options = SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal)
            .foreign_keys(true);
        Self::connect(SqlitePoolOptions::new().connect_with(options).await?).await
    }

    /// A private database for tests; one connection so every query sees the same data.
    pub async fn in_memory() -> Result<Self, StoreError> {
        let options = SqliteConnectOptions::from_str("sqlite::memory:")?.foreign_keys(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await?;
        Self::connect(pool).await
    }

    async fn connect(pool: SqlitePool) -> Result<Self, StoreError> {
        sqlx::migrate!("./migrations").run(&pool).await?;
        Ok(Self { pool })
    }

    pub async fn create_task(&self, task: &Task) -> Result<(), StoreError> {
        let now = Utc::now().to_rfc3339();
        let result = sqlx::query(
            "INSERT INTO tasks (key, title, workflow, phase, status, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(task.key.as_str())
        .bind(&task.title)
        .bind(&task.workflow)
        .bind(&task.phase)
        .bind(serde_json::to_string(&task.status)?)
        .bind(&now)
        .bind(&now)
        .execute(&self.pool)
        .await;
        match result {
            Err(sqlx::Error::Database(e)) if e.is_unique_violation() => {
                Err(StoreError::AlreadyExists(task.key.clone()))
            }
            other => other.map(|_| ()).map_err(Into::into),
        }
    }

    pub async fn get_task(&self, key: &TaskKey) -> Result<Option<Task>, StoreError> {
        let row =
            sqlx::query("SELECT key, title, workflow, phase, status FROM tasks WHERE key = ?")
                .bind(key.as_str())
                .fetch_optional(&self.pool)
                .await?;
        row.as_ref().map(task_from_row).transpose()
    }

    pub async fn list_tasks(&self) -> Result<Vec<Task>, StoreError> {
        let rows = sqlx::query(
            "SELECT key, title, workflow, phase, status FROM tasks ORDER BY created_at, key",
        )
        .fetch_all(&self.pool)
        .await?;
        rows.iter().map(task_from_row).collect()
    }

    /// Applies `event` through the state machine and records it, atomically.
    /// A rejected event changes nothing and is not recorded.
    pub async fn apply_event(
        &self,
        key: &TaskKey,
        event: Event,
        at: DateTime<Utc>,
    ) -> Result<Task, StoreError> {
        let mut tx = self.pool.begin().await?;
        let row =
            sqlx::query("SELECT key, title, workflow, phase, status FROM tasks WHERE key = ?")
                .bind(key.as_str())
                .fetch_optional(&mut *tx)
                .await?
                .ok_or_else(|| StoreError::NotFound(key.clone()))?;
        let mut task = task_from_row(&row)?;

        task.apply(event.clone())?;

        sqlx::query("UPDATE tasks SET phase = ?, status = ?, updated_at = ? WHERE key = ?")
            .bind(&task.phase)
            .bind(serde_json::to_string(&task.status)?)
            .bind(at.to_rfc3339())
            .bind(key.as_str())
            .execute(&mut *tx)
            .await?;
        sqlx::query("INSERT INTO task_events (task_key, at, event) VALUES (?, ?, ?)")
            .bind(key.as_str())
            .bind(at.to_rfc3339())
            .bind(serde_json::to_string(&event)?)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(task)
    }

    pub async fn events(&self, key: &TaskKey) -> Result<Vec<EventRecord>, StoreError> {
        if self.get_task(key).await?.is_none() {
            return Err(StoreError::NotFound(key.clone()));
        }
        let rows = sqlx::query("SELECT at, event FROM task_events WHERE task_key = ? ORDER BY id")
            .bind(key.as_str())
            .fetch_all(&self.pool)
            .await?;
        rows.iter()
            .map(|row| {
                let at: String = row.try_get("at")?;
                let event: String = row.try_get("event")?;
                Ok(EventRecord {
                    at: DateTime::parse_from_rfc3339(&at)
                        .map_err(|e| sqlx::Error::Decode(Box::new(e)))?
                        .with_timezone(&Utc),
                    event: serde_json::from_str(&event)?,
                })
            })
            .collect()
    }
}

fn task_from_row(row: &SqliteRow) -> Result<Task, StoreError> {
    let status: String = row.try_get("status")?;
    Ok(Task {
        key: TaskKey::new(row.try_get::<String, _>("key")?),
        title: row.try_get("title")?,
        workflow: row.try_get("workflow")?,
        phase: row.try_get("phase")?,
        status: serde_json::from_str::<TaskStatus>(&status)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn task(key: &str) -> Task {
        Task::new(TaskKey::new(key), "Card retries", "feature")
    }

    #[tokio::test]
    async fn keeps_tasks_and_rejects_duplicates() {
        let store = Store::in_memory().await.unwrap();
        store.create_task(&task("PROJ-1")).await.unwrap();
        store.create_task(&task("PROJ-2")).await.unwrap();

        let err = store.create_task(&task("PROJ-1")).await.unwrap_err();
        assert!(matches!(err, StoreError::AlreadyExists(_)), "{err}");

        let keys: Vec<_> = store
            .list_tasks()
            .await
            .unwrap()
            .into_iter()
            .map(|t| t.key)
            .collect();
        assert_eq!(keys, [TaskKey::new("PROJ-1"), TaskKey::new("PROJ-2")]);
    }

    #[tokio::test]
    async fn applies_events_and_keeps_their_history() {
        let store = Store::in_memory().await.unwrap();
        let key = TaskKey::new("PROJ-1");
        store.create_task(&task("PROJ-1")).await.unwrap();

        let now = Utc::now();
        store.apply_event(&key, Event::Start, now).await.unwrap();
        store
            .apply_event(&key, Event::WorkspaceReady, now)
            .await
            .unwrap();
        let resume_at = now + chrono::Duration::hours(1);
        let task = store
            .apply_event(&key, Event::LimitReached { resume_at }, now)
            .await
            .unwrap();

        assert_eq!(task.status, TaskStatus::PausedByLimit { resume_at });
        assert_eq!(store.get_task(&key).await.unwrap(), Some(task));
        let history: Vec<_> = store
            .events(&key)
            .await
            .unwrap()
            .into_iter()
            .map(|r| r.event)
            .collect();
        assert_eq!(
            history,
            [
                Event::Start,
                Event::WorkspaceReady,
                Event::LimitReached { resume_at }
            ]
        );
    }

    #[tokio::test]
    async fn rejected_events_change_nothing() {
        let store = Store::in_memory().await.unwrap();
        let key = TaskKey::new("PROJ-1");
        store.create_task(&task("PROJ-1")).await.unwrap();

        let err = store
            .apply_event(&key, Event::AgentFinished, Utc::now())
            .await
            .unwrap_err();
        assert!(matches!(err, StoreError::Transition(_)), "{err}");
        assert_eq!(
            store.get_task(&key).await.unwrap().unwrap().status,
            TaskStatus::Queued
        );
        assert!(store.events(&key).await.unwrap().is_empty());

        let missing = TaskKey::new("PROJ-404");
        let err = store
            .apply_event(&missing, Event::Start, Utc::now())
            .await
            .unwrap_err();
        assert!(matches!(err, StoreError::NotFound(_)), "{err}");
    }
}
