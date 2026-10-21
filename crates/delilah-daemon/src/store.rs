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
    #[error("task {0} has already been started; only backlog tasks can be edited")]
    NotInBacklog(TaskKey),
    #[error("project `{0}` does not exist; add it with `delilah project add`")]
    ProjectNotFound(String),
    #[error("project `{0}` already exists (same name or folder)")]
    ProjectExists(String),
    #[error(transparent)]
    Transition(#[from] TransitionError),
    #[error("database error: {0}")]
    Db(#[from] sqlx::Error),
    #[error("migration error: {0}")]
    Migrate(#[from] sqlx::migrate::MigrateError),
    #[error("corrupt stored value: {0}")]
    Json(#[from] serde_json::Error),
    #[error("corrupt stored value: {0}")]
    Corrupt(String),
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
    pub(crate) pool: SqlitePool,
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
            "INSERT INTO tasks (key, title, project, workflow, phase, status, notes, in_checkout, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(task.key.as_str())
        .bind(&task.title)
        .bind(&task.project)
        .bind(&task.workflow)
        .bind(&task.phase)
        .bind(serde_json::to_string(&task.status)?)
        .bind(&task.notes)
        .bind(task.in_checkout)
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
        let row = sqlx::query(
            "SELECT key, title, project, workflow, phase, status, notes, in_checkout FROM tasks WHERE key = ?",
        )
        .bind(key.as_str())
        .fetch_optional(&self.pool)
        .await?;
        row.as_ref().map(task_from_row).transpose()
    }

    pub async fn list_tasks(&self) -> Result<Vec<Task>, StoreError> {
        let rows = sqlx::query(
            "SELECT key, title, project, workflow, phase, status, notes, in_checkout FROM tasks ORDER BY created_at, key",
        )
        .fetch_all(&self.pool)
        .await?;
        rows.iter().map(task_from_row).collect()
    }

    /// Changes what a backlog task says before it starts: each `Some` field is set
    /// (`notes: Some("")` clears them).
    pub async fn update_backlog_task(
        &self,
        key: &TaskKey,
        change: &BacklogChange,
    ) -> Result<Task, StoreError> {
        let task = self
            .get_task(key)
            .await?
            .ok_or_else(|| StoreError::NotFound(key.clone()))?;
        if task.status != TaskStatus::Queued {
            return Err(StoreError::NotInBacklog(key.clone()));
        }
        if let Some(project) = &change.project {
            self.project(project).await?;
        }
        let notes = change
            .notes
            .as_ref()
            .map(|n| Some(n.clone()).filter(|n| !n.trim().is_empty()));
        // The status in the WHERE: a start that slipped in between wins.
        let updated = sqlx::query(
            "UPDATE tasks SET title = ?, project = ?, workflow = ?, notes = ?, in_checkout = ?, updated_at = ?
             WHERE key = ? AND status = ?",
        )
        .bind(change.title.as_ref().unwrap_or(&task.title))
        .bind(change.project.as_ref().or(task.project.as_ref()))
        .bind(change.workflow.as_ref().unwrap_or(&task.workflow))
        .bind(notes.unwrap_or(task.notes))
        .bind(change.in_checkout.or(task.in_checkout))
        .bind(Utc::now().to_rfc3339())
        .bind(key.as_str())
        .bind(serde_json::to_string(&TaskStatus::Queued)?)
        .execute(&self.pool)
        .await?;
        if updated.rows_affected() == 0 {
            return Err(StoreError::NotInBacklog(key.clone()));
        }
        Ok(self.get_task(key).await?.expect("task was just updated"))
    }

    /// Removes a task for good. Its events, workspace record and sessions go with it
    /// (`ON DELETE CASCADE`), in the same statement: it is either all there or all gone.
    pub async fn delete_task(&self, key: &TaskKey) -> Result<(), StoreError> {
        let deleted = sqlx::query("DELETE FROM tasks WHERE key = ?")
            .bind(key.as_str())
            .execute(&self.pool)
            .await?;
        if deleted.rows_affected() == 0 {
            return Err(StoreError::NotFound(key.clone()));
        }
        Ok(())
    }

    /// Removes a task only while it is still in the backlog (`NotInBacklog` once a
    /// start got there first).
    pub async fn delete_backlog_task(&self, key: &TaskKey) -> Result<(), StoreError> {
        let deleted = sqlx::query("DELETE FROM tasks WHERE key = ? AND status = ?")
            .bind(key.as_str())
            .bind(serde_json::to_string(&TaskStatus::Queued)?)
            .execute(&self.pool)
            .await?;
        if deleted.rows_affected() == 0 {
            return Err(match self.get_task(key).await? {
                Some(_) => StoreError::NotInBacklog(key.clone()),
                None => StoreError::NotFound(key.clone()),
            });
        }
        Ok(())
    }

    /// Applies `event` through the state machine and records it, atomically.
    /// A rejected event changes nothing and is not recorded.
    pub async fn apply_event(
        &self,
        key: &TaskKey,
        event: Event,
        at: DateTime<Utc>,
    ) -> Result<Task, StoreError> {
        // IMMEDIATE takes the write lock before reading: two events on the same task
        // (e.g. two starts at once) apply one after the other, the second against the
        // first's result, instead of the second failing with SQLITE_BUSY.
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let row = sqlx::query(
            "SELECT key, title, project, workflow, phase, status, notes, in_checkout FROM tasks WHERE key = ?",
        )
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

/// What `PATCH /api/v1/tasks/{key}` may change in a backlog task.
#[derive(Debug, Default, serde::Deserialize)]
pub struct BacklogChange {
    pub title: Option<String>,
    pub notes: Option<String>,
    pub project: Option<String>,
    #[serde(default)]
    pub workflow: Option<String>,
    /// Where it will work once started: the project's checkout or a worktree.
    pub in_checkout: Option<bool>,
}

fn task_from_row(row: &SqliteRow) -> Result<Task, StoreError> {
    let status: String = row.try_get("status")?;
    Ok(Task {
        key: TaskKey::new(row.try_get::<String, _>("key")?),
        title: row.try_get("title")?,
        project: row.try_get("project")?,
        workflow: row.try_get("workflow")?,
        phase: row.try_get("phase")?,
        status: serde_json::from_str::<TaskStatus>(&status)?,
        notes: row.try_get("notes")?,
        in_checkout: row.try_get("in_checkout")?,
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
        let mut noted = task("PROJ-2");
        noted.notes = Some("Mirar los reintentos\n- con backoff".into());
        store.create_task(&noted).await.unwrap();
        assert_eq!(store.get_task(&noted.key).await.unwrap(), Some(noted));

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

    #[tokio::test]
    async fn a_deleted_task_takes_its_history_workspace_and_sessions() {
        let store = Store::in_memory().await.unwrap();
        let key = TaskKey::new("PROJ-1");
        store.create_task(&task("PROJ-1")).await.unwrap();
        store
            .apply_event(&key, Event::Start, Utc::now())
            .await
            .unwrap();
        let ws = delilah_git::Workspace {
            path: "/w/ws/PROJ-1".into(),
            branch: "feature/PROJ-1".into(),
            kind: delilah_core::ProjectKind::Repo,
            submodules: vec![],
        };
        store
            .save_workspace(&key, Path::new("/w/web"), &ws)
            .await
            .unwrap();
        store
            .create_session(&crate::store_sessions::SessionRecord {
                id: "s-1".into(),
                task: "PROJ-1".into(),
                kind: crate::store_sessions::SessionKind::Claude,
                label: "Principal".into(),
                primary: true,
                closed: false,
                read_only: false,
            })
            .await
            .unwrap();

        store.delete_task(&key).await.unwrap();
        assert!(store.get_task(&key).await.unwrap().is_none());
        let events: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM task_events WHERE task_key = ?")
            .bind("PROJ-1")
            .fetch_one(&store.pool)
            .await
            .unwrap();
        assert_eq!(events, 0);
        assert!(store.workspace(&key).await.unwrap().is_none());
        assert!(store.session("s-1").await.unwrap().is_none());

        let err = store.delete_task(&key).await.unwrap_err();
        assert!(matches!(err, StoreError::NotFound(_)), "{err}");
    }

    #[tokio::test]
    async fn two_starts_at_once_start_the_task_once() {
        // On disk, with a pool of connections, as the daemon runs.
        let dir = std::env::temp_dir().join(format!("delilah-store-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let store = Store::open(&dir.join("delilah.db")).await.unwrap();
        let key = TaskKey::new("PROJ-1");
        store.create_task(&task("PROJ-1")).await.unwrap();

        let (a, b) = tokio::join!(
            store.apply_event(&key, Event::Start, Utc::now()),
            store.apply_event(&key, Event::Start, Utc::now()),
        );
        let errors: Vec<_> = [a, b].into_iter().filter_map(Result::err).collect();
        assert_eq!(errors.len(), 1, "{errors:?}");
        assert!(
            matches!(errors[0], StoreError::Transition(_)),
            "{}",
            errors[0]
        );
        assert_eq!(store.events(&key).await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn edits_backlog_tasks_only() {
        let store = Store::in_memory().await.unwrap();
        store
            .create_project(&delilah_core::Project {
                name: "web".into(),
                path: "/w/web".into(),
                kind: delilah_core::ProjectKind::Repo,
                workflow: "feature".into(),
                settings: Default::default(),
            })
            .await
            .unwrap();
        let key = TaskKey::new("PROJ-1");
        store.create_task(&task("PROJ-1")).await.unwrap();

        let change = BacklogChange {
            title: Some("Reintentos".into()),
            notes: Some("con backoff".into()),
            project: Some("web".into()),
            workflow: Some("bugfix".into()),
            in_checkout: Some(false),
        };
        let task = store.update_backlog_task(&key, &change).await.unwrap();
        assert_eq!(task.title, "Reintentos");
        assert_eq!(task.in_checkout, Some(false), "in a worktree");
        assert_eq!(task.notes.as_deref(), Some("con backoff"));
        assert_eq!(task.project.as_deref(), Some("web"));
        assert_eq!(task.workflow, "bugfix", "a start may change the workflow");

        // Empty notes clear them; fields left out stay.
        let clear = BacklogChange {
            notes: Some("  ".into()),
            ..Default::default()
        };
        let task = store.update_backlog_task(&key, &clear).await.unwrap();
        assert_eq!((task.title.as_str(), task.notes), ("Reintentos", None));
        assert_eq!(task.in_checkout, Some(false), "left out, it stays");

        let nowhere = BacklogChange {
            project: Some("nope".into()),
            ..Default::default()
        };
        let err = store.update_backlog_task(&key, &nowhere).await.unwrap_err();
        assert!(matches!(err, StoreError::ProjectNotFound(_)), "{err}");

        store
            .apply_event(&key, Event::Start, Utc::now())
            .await
            .unwrap();
        let err = store.update_backlog_task(&key, &change).await.unwrap_err();
        assert!(matches!(err, StoreError::NotInBacklog(_)), "{err}");
    }
}
