use std::path::Path;
use std::str::FromStr;

use chrono::{DateTime, Utc};
use delilah_core::{
    Autopilot, Event, Handoff, Task, TaskKey, TaskStatus, TaskView, TitleSource, TransitionError,
};
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
        insert_task(&self.pool, task).await
    }

    /// Creates a task and records, in the same transaction, that it came from a
    /// conversation: either both exist or neither does.
    pub async fn create_task_from_conversation(
        &self,
        task: &Task,
        session: &str,
        label: &str,
        at: DateTime<Utc>,
    ) -> Result<(), StoreError> {
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        insert_task(&mut *tx, task).await?;
        let event = Event::CreatedFromConversation {
            session: session.to_string(),
            label: label.to_string(),
        };
        sqlx::query("INSERT INTO task_events (task_key, at, event) VALUES (?, ?, ?)")
            .bind(task.key.as_str())
            .bind(at.to_rfc3339())
            .bind(serde_json::to_string(&event)?)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn get_task(&self, key: &TaskKey) -> Result<Option<Task>, StoreError> {
        let row = sqlx::query(
            "SELECT t.key, t.title, t.project, t.workflow, t.start_message, t.phase, t.status, t.notes, t.in_checkout, t.saving_mode, t.handoff, t.autopilot, t.title_source, t.previous_title, t.branch_error, t.view, t.chat_mode, t.allowed_tools, t.created_at, NULLIF(w.branch, '') AS branch, (SELECT e.at FROM task_events e WHERE e.task_key = t.key AND json_extract(e.event, '$.type') IN ('mark-done', 'agent-finished') ORDER BY e.id DESC LIMIT 1) AS done_at
             FROM tasks t LEFT JOIN workspaces w ON w.task_key = t.key WHERE t.key = ?",
        )
        .bind(key.as_str())
        .fetch_optional(&self.pool)
        .await?;
        row.as_ref().map(task_from_row).transpose()
    }

    pub async fn list_tasks(&self) -> Result<Vec<Task>, StoreError> {
        let rows = sqlx::query(
            "SELECT t.key, t.title, t.project, t.workflow, t.start_message, t.phase, t.status, t.notes, t.in_checkout, t.saving_mode, t.handoff, t.autopilot, t.title_source, t.previous_title, t.branch_error, t.view, t.chat_mode, t.allowed_tools, t.created_at, NULLIF(w.branch, '') AS branch, (SELECT e.at FROM task_events e WHERE e.task_key = t.key AND json_extract(e.event, '$.type') IN ('mark-done', 'agent-finished') ORDER BY e.id DESC LIMIT 1) AS done_at
             FROM tasks t LEFT JOIN workspaces w ON w.task_key = t.key ORDER BY t.created_at, t.key",
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
        // A title the user changes is theirs from then on.
        let title_changed = change.title.as_ref().is_some_and(|t| *t != task.title);
        // The status in the WHERE: a start that slipped in between wins.
        let updated = sqlx::query(
            "UPDATE tasks SET title = ?, title_source = ?, previous_title = ?, project = ?, workflow = ?, notes = ?, in_checkout = ?, updated_at = ?
             WHERE key = ? AND status = ?",
        )
        .bind(change.title.as_ref().unwrap_or(&task.title))
        .bind(
            if title_changed {
                TitleSource::Manual
            } else {
                task.title_source
            }
            .name(),
        )
        .bind(if title_changed {
            None
        } else {
            task.previous_title.as_ref()
        })
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

    /// Sets (or clears) the message a `prompt` task starts with. Not part of
    /// `BacklogChange`, which callers build field by field.
    pub async fn set_start_message(
        &self,
        key: &TaskKey,
        message: Option<&str>,
    ) -> Result<(), StoreError> {
        let updated =
            sqlx::query("UPDATE tasks SET start_message = ?, updated_at = ? WHERE key = ?")
                .bind(message)
                .bind(Utc::now().to_rfc3339())
                .bind(key.as_str())
                .execute(&self.pool)
                .await?;
        if updated.rows_affected() == 0 {
            return Err(StoreError::NotFound(key.clone()));
        }
        Ok(())
    }

    /// Turns saving mode on or off for a task, whatever state it is in. Turning it
    /// off also turns autopilot off, which needs it.
    pub async fn set_saving_mode(&self, key: &TaskKey, on: bool) -> Result<(), StoreError> {
        let updated = sqlx::query(
            "UPDATE tasks SET saving_mode = ?1, autopilot = CASE WHEN ?1 THEN autopilot END, updated_at = ?2 WHERE key = ?3",
        )
            .bind(on)
            .bind(Utc::now().to_rfc3339())
            .bind(key.as_str())
            .execute(&self.pool)
            .await?;
        if updated.rows_affected() == 0 {
            return Err(StoreError::NotFound(key.clone()));
        }
        Ok(())
    }

    /// Sets how the app shows a task, whatever state it is in; returns the task.
    pub async fn set_view(&self, key: &TaskKey, view: TaskView) -> Result<Task, StoreError> {
        let updated = sqlx::query("UPDATE tasks SET view = ?, updated_at = ? WHERE key = ?")
            .bind(view.as_str())
            .bind(Utc::now().to_rfc3339())
            .bind(key.as_str())
            .execute(&self.pool)
            .await?;
        if updated.rows_affected() == 0 {
            return Err(StoreError::NotFound(key.clone()));
        }
        self.get_task(key)
            .await?
            .ok_or_else(|| StoreError::NotFound(key.clone()))
    }

    /// Sets the runtime of a task's Claude sessions (chat or terminal); returns the task.
    pub async fn set_chat_mode(&self, key: &TaskKey, chat_mode: bool) -> Result<Task, StoreError> {
        let updated = sqlx::query("UPDATE tasks SET chat_mode = ?, updated_at = ? WHERE key = ?")
            .bind(chat_mode)
            .bind(Utc::now().to_rfc3339())
            .bind(key.as_str())
            .execute(&self.pool)
            .await?;
        if updated.rows_affected() == 0 {
            return Err(StoreError::NotFound(key.clone()));
        }
        self.get_task(key)
            .await?
            .ok_or_else(|| StoreError::NotFound(key.clone()))
    }

    /// Adds a permission rule to the task's allowed tools (no duplicates); returns the task.
    pub async fn add_allowed_tool(&self, key: &TaskKey, rule: &str) -> Result<Task, StoreError> {
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let json: Option<String> =
            sqlx::query_scalar("SELECT allowed_tools FROM tasks WHERE key = ?")
                .bind(key.as_str())
                .fetch_optional(&mut *tx)
                .await?
                .ok_or_else(|| StoreError::NotFound(key.clone()))?;
        let mut tools = allowed_tools_from_json(json.as_deref());
        if !tools.iter().any(|t| t == rule) {
            tools.push(rule.to_string());
            sqlx::query("UPDATE tasks SET allowed_tools = ?, updated_at = ? WHERE key = ?")
                .bind(allowed_tools_json(&tools)?)
                .bind(Utc::now().to_rfc3339())
                .bind(key.as_str())
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await?;
        self.get_task(key)
            .await?
            .ok_or_else(|| StoreError::NotFound(key.clone()))
    }

    /// Records where the title came from and the title it replaced (if any).
    pub async fn set_title_source(
        &self,
        key: &TaskKey,
        source: TitleSource,
        previous_title: Option<&str>,
    ) -> Result<(), StoreError> {
        let updated = sqlx::query(
            "UPDATE tasks SET title_source = ?, previous_title = ?, updated_at = ? WHERE key = ?",
        )
        .bind(source.name())
        .bind(previous_title)
        .bind(Utc::now().to_rfc3339())
        .bind(key.as_str())
        .execute(&self.pool)
        .await?;
        if updated.rows_affected() == 0 {
            return Err(StoreError::NotFound(key.clone()));
        }
        Ok(())
    }

    /// Records the autopilot of a task, or clears it. Turning it on also turns saving
    /// mode on, in the same statement; clearing it leaves saving mode as it is.
    ///
    /// Unconditional: only for creating or starting a task. Any later change is
    /// [`Store::update_autopilot`], so it never overwrites a pause or an "off" (#45).
    pub async fn set_autopilot(
        &self,
        key: &TaskKey,
        autopilot: Option<&Autopilot>,
    ) -> Result<(), StoreError> {
        let json = autopilot.map(serde_json::to_string).transpose()?;
        let updated = sqlx::query(
            "UPDATE tasks SET autopilot = ?1, saving_mode = CASE WHEN ?1 IS NOT NULL THEN 1 ELSE saving_mode END, updated_at = ?2 WHERE key = ?3",
        )
        .bind(json)
        .bind(Utc::now().to_rfc3339())
        .bind(key.as_str())
        .execute(&self.pool)
        .await?;
        if updated.rows_affected() == 0 {
            return Err(StoreError::NotFound(key.clone()));
        }
        Ok(())
    }

    /// Records why the task's branch couldn't be created, or clears it.
    pub async fn set_branch_error(
        &self,
        key: &TaskKey,
        error: Option<&str>,
    ) -> Result<(), StoreError> {
        let updated =
            sqlx::query("UPDATE tasks SET branch_error = ?, updated_at = ? WHERE key = ?")
                .bind(error)
                .bind(Utc::now().to_rfc3339())
                .bind(key.as_str())
                .execute(&self.pool)
                .await?;
        if updated.rows_affected() == 0 {
            return Err(StoreError::NotFound(key.clone()));
        }
        Ok(())
    }

    /// The user's own title for a task in any state but archived (finished and its
    /// workspace released, which is a conflict): the title becomes `manual` and the
    /// replaced one is forgotten.
    pub async fn set_title(&self, key: &TaskKey, title: &str) -> Result<Task, StoreError> {
        let task = self
            .get_task(key)
            .await?
            .ok_or_else(|| StoreError::NotFound(key.clone()))?;
        if task.status.is_terminal() && self.workspace(key).await?.is_none() {
            return Err(StoreError::Transition(TransitionError::Invalid {
                status: task.status.name(),
                event: "rename",
            }));
        }
        sqlx::query(
            "UPDATE tasks SET title = ?, title_source = ?, previous_title = NULL, updated_at = ? WHERE key = ?",
        )
        .bind(title)
        .bind(TitleSource::Manual.name())
        .bind(Utc::now().to_rfc3339())
        .bind(key.as_str())
        .execute(&self.pool)
        .await?;
        Ok(self.get_task(key).await?.expect("task was just updated"))
    }

    /// The automatic rename from the first analysis: only while the title is still the
    /// initial one, so it happens at most once. The replaced title is kept. `None` when
    /// the task was not (or no longer) `initial`.
    pub async fn rename_from_spec(
        &self,
        key: &TaskKey,
        title: &str,
    ) -> Result<Option<Task>, StoreError> {
        let updated = sqlx::query(
            "UPDATE tasks SET previous_title = title, title = ?, title_source = ?, updated_at = ?
             WHERE key = ? AND title_source = ?",
        )
        .bind(title)
        .bind(TitleSource::Spec.name())
        .bind(Utc::now().to_rfc3339())
        .bind(key.as_str())
        .bind(TitleSource::Initial.name())
        .execute(&self.pool)
        .await?;
        if updated.rows_affected() == 0 {
            return Ok(None);
        }
        self.get_task(key).await
    }

    /// Changes a task's autopilot as a compare-and-set: `f` gets the stored value and
    /// returns the new one (`Some(None)` clears it), or `None` to leave it alone. The
    /// write only lands if the stored value is still the one `f` saw, else `f` runs
    /// again on the fresh value, so a concurrent change is never overwritten with a
    /// stale copy. `true` if something was written.
    pub async fn update_autopilot<F>(&self, key: &TaskKey, mut f: F) -> Result<bool, StoreError>
    where
        F: FnMut(Option<Autopilot>) -> Option<Option<Autopilot>>,
    {
        for _ in 0..32 {
            let row = sqlx::query("SELECT autopilot FROM tasks WHERE key = ?")
                .bind(key.as_str())
                .fetch_optional(&self.pool)
                .await?
                .ok_or_else(|| StoreError::NotFound(key.clone()))?;
            let stored: Option<String> = row.try_get("autopilot")?;
            let current = stored
                .as_deref()
                .map(serde_json::from_str::<Autopilot>)
                .transpose()?;
            let Some(new) = f(current) else {
                return Ok(false);
            };
            let json = new.as_ref().map(serde_json::to_string).transpose()?;
            let updated = sqlx::query(
                "UPDATE tasks SET autopilot = ?1, saving_mode = CASE WHEN ?1 IS NOT NULL THEN 1 ELSE saving_mode END, updated_at = ?2 WHERE key = ?3 AND autopilot IS ?4",
            )
            .bind(json)
            .bind(Utc::now().to_rfc3339())
            .bind(key.as_str())
            .bind(stored)
            .execute(&self.pool)
            .await?;
            if updated.rows_affected() == 1 {
                return Ok(true);
            }
        }
        Err(StoreError::Corrupt(format!(
            "the autopilot of {key} keeps changing"
        )))
    }

    /// Records the handoff under way, or clears it.
    pub async fn set_handoff(
        &self,
        key: &TaskKey,
        handoff: Option<&Handoff>,
    ) -> Result<(), StoreError> {
        let json = handoff.map(serde_json::to_string).transpose()?;
        let updated = sqlx::query("UPDATE tasks SET handoff = ?, updated_at = ? WHERE key = ?")
            .bind(json)
            .bind(Utc::now().to_rfc3339())
            .bind(key.as_str())
            .execute(&self.pool)
            .await?;
        if updated.rows_affected() == 0 {
            return Err(StoreError::NotFound(key.clone()));
        }
        Ok(())
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
            "SELECT t.key, t.title, t.project, t.workflow, t.start_message, t.phase, t.status, t.notes, t.in_checkout, t.saving_mode, t.handoff, t.autopilot, t.title_source, t.previous_title, t.branch_error, t.view, t.chat_mode, t.allowed_tools, t.created_at, NULLIF(w.branch, '') AS branch, (SELECT e.at FROM task_events e WHERE e.task_key = t.key AND json_extract(e.event, '$.type') IN ('mark-done', 'agent-finished') ORDER BY e.id DESC LIMIT 1) AS done_at
             FROM tasks t LEFT JOIN workspaces w ON w.task_key = t.key WHERE t.key = ?",
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
        // Done by this very event, or still done from an earlier one.
        task.finished_at = match (&task.status, &event) {
            (TaskStatus::Done, Event::MarkDone | Event::AgentFinished) => Some(at),
            (TaskStatus::Done, _) => task.finished_at,
            _ => None,
        };
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

async fn insert_task<'e>(
    executor: impl sqlx::Executor<'e, Database = sqlx::Sqlite>,
    task: &Task,
) -> Result<(), StoreError> {
    let now = Utc::now().to_rfc3339();
    let result = sqlx::query(
        "INSERT INTO tasks (key, title, project, workflow, start_message, phase, status, notes, in_checkout, saving_mode, handoff, autopilot, title_source, previous_title, branch_error, view, chat_mode, allowed_tools, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(task.key.as_str())
    .bind(&task.title)
    .bind(&task.project)
    .bind(&task.workflow)
    .bind(&task.start_message)
    .bind(&task.phase)
    .bind(serde_json::to_string(&task.status)?)
    .bind(&task.notes)
    .bind(task.in_checkout)
    .bind(task.saving_mode)
    .bind(task.handoff.as_ref().map(serde_json::to_string).transpose()?)
    .bind(task.autopilot.as_ref().map(serde_json::to_string).transpose()?)
    .bind(task.title_source.name())
    .bind(&task.previous_title)
    .bind(&task.branch_error)
    .bind(task.view.as_str())
    .bind(task.chat_mode)
    .bind(allowed_tools_json(&task.allowed_tools)?)
    .bind(task.created_at.to_rfc3339())
    .bind(&now)
    .execute(executor)
    .await;
    match result {
        Err(sqlx::Error::Database(e)) if e.is_unique_violation() => {
            Err(StoreError::AlreadyExists(task.key.clone()))
        }
        other => other.map(|_| ()).map_err(Into::into),
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

impl BacklogChange {
    pub fn is_empty(&self) -> bool {
        self.title.is_none()
            && self.notes.is_none()
            && self.project.is_none()
            && self.workflow.is_none()
            && self.in_checkout.is_none()
    }
}

/// `None` (NULL) when there are no rules.
fn allowed_tools_json(tools: &[String]) -> Result<Option<String>, StoreError> {
    if tools.is_empty() {
        return Ok(None);
    }
    Ok(Some(serde_json::to_string(tools)?))
}

/// NULL is no rules; JSON that can't be read is logged and treated as no rules.
fn allowed_tools_from_json(json: Option<&str>) -> Vec<String> {
    let Some(json) = json else {
        return Vec::new();
    };
    serde_json::from_str(json).unwrap_or_else(|err| {
        tracing::warn!("allowed_tools is not a JSON list of strings, ignoring it: {err}");
        Vec::new()
    })
}

fn task_from_row(row: &SqliteRow) -> Result<Task, StoreError> {
    let status: String = row.try_get("status")?;
    let task_status = serde_json::from_str::<TaskStatus>(&status)?;
    Ok(Task {
        key: TaskKey::new(row.try_get::<String, _>("key")?),
        title: row.try_get("title")?,
        project: row.try_get("project")?,
        workflow: row.try_get("workflow")?,
        phase: row.try_get("phase")?,
        status: task_status.clone(),
        notes: row.try_get("notes")?,
        start_message: row.try_get("start_message")?,
        in_checkout: row.try_get("in_checkout")?,
        branch: row.try_get("branch")?,
        saving_mode: row.try_get("saving_mode")?,
        handoff: row
            .try_get::<Option<String>, _>("handoff")?
            .map(|json| serde_json::from_str(&json))
            .transpose()?,
        autopilot: row
            .try_get::<Option<String>, _>("autopilot")?
            .map(|json| serde_json::from_str(&json))
            .transpose()?,
        title_source: {
            let source: String = row.try_get("title_source")?;
            TitleSource::parse(&source)
                .ok_or_else(|| StoreError::Corrupt(format!("unknown title source `{source}`")))?
        },
        previous_title: row.try_get("previous_title")?,
        branch_error: row.try_get("branch_error")?,
        view: TaskView::parse(&row.try_get::<String, _>("view")?).unwrap_or_default(),
        chat_mode: row.try_get("chat_mode")?,
        allowed_tools: allowed_tools_from_json(
            row.try_get::<Option<String>, _>("allowed_tools")?
                .as_deref(),
        ),
        created_at: {
            let text: String = row.try_get("created_at")?;
            DateTime::parse_from_rfc3339(&text)
                .map(|t| t.with_timezone(&Utc))
                .unwrap_or_else(|err| {
                    tracing::warn!("created_at `{text}` is not a date, using now: {err}");
                    Utc::now()
                })
        },
        // The time of the event that made it done; only while it is.
        finished_at: if matches!(task_status, TaskStatus::Done) {
            row.try_get::<Option<String>, _>("done_at")?
                .and_then(|t| DateTime::parse_from_rfc3339(&t).ok())
                .map(|t| t.with_timezone(&Utc))
        } else {
            None
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn tasks_carry_their_stored_created_at() {
        let store = Store::in_memory().await.unwrap();
        let mut t = task("PROJ-1");
        t.created_at = "2026-01-02T03:04:05Z".parse().unwrap();
        store.create_task(&t).await.unwrap();
        let got = store
            .get_task(&TaskKey::new("PROJ-1"))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(got.created_at, t.created_at);
        assert_eq!(
            store.list_tasks().await.unwrap()[0].created_at,
            t.created_at
        );
    }

    fn task(key: &str) -> Task {
        Task::new(TaskKey::new(key), "Card retries", "project")
    }

    #[tokio::test]
    async fn a_done_task_carries_when_it_finished() {
        let store = Store::in_memory().await.unwrap();
        let key = TaskKey::new("PROJ-1");
        store.create_task(&task("PROJ-1")).await.unwrap();
        let t0: DateTime<Utc> = "2026-02-03T04:05:06Z".parse().unwrap();
        let t1: DateTime<Utc> = "2026-02-03T05:00:00Z".parse().unwrap();
        for e in [Event::Start, Event::WorkspaceReady] {
            store.apply_event(&key, e, t0).await.unwrap();
        }
        assert_eq!(
            store.get_task(&key).await.unwrap().unwrap().finished_at,
            None
        );
        let done = store.apply_event(&key, Event::MarkDone, t1).await.unwrap();
        assert_eq!(done.finished_at, Some(t1));
        assert_eq!(
            store.get_task(&key).await.unwrap().unwrap().finished_at,
            Some(t1)
        );
        assert_eq!(store.list_tasks().await.unwrap()[0].finished_at, Some(t1));
        // Reopened: not finished any more.
        let t2: DateTime<Utc> = "2026-02-04T05:00:00Z".parse().unwrap();
        let reopened = store.apply_event(&key, Event::Reopen, t2).await.unwrap();
        assert_eq!(reopened.finished_at, None);
        assert_eq!(
            store.get_task(&key).await.unwrap().unwrap().finished_at,
            None
        );
    }

    #[tokio::test]
    async fn chat_mode_and_allowed_tools_persist() {
        let store = Store::in_memory().await.unwrap();
        let key = TaskKey::new("PROJ-1");
        let mut chat = task("PROJ-1");
        chat.chat_mode = true;
        chat.allowed_tools = vec!["Read".into()];
        store.create_task(&chat).await.unwrap();
        assert_eq!(store.get_task(&key).await.unwrap(), Some(chat));
        assert_eq!(store.list_tasks().await.unwrap()[0].allowed_tools, ["Read"]);

        let t = store.set_chat_mode(&key, false).await.unwrap();
        assert!(!t.chat_mode);
        let t = store.add_allowed_tool(&key, "Bash(ls:*)").await.unwrap();
        assert_eq!(t.allowed_tools, ["Read", "Bash(ls:*)"]);
        let t = store.add_allowed_tool(&key, "Read").await.unwrap();
        assert_eq!(t.allowed_tools, ["Read", "Bash(ls:*)"]);

        let missing = TaskKey::new("NOPE-1");
        assert!(matches!(
            store.set_chat_mode(&missing, true).await,
            Err(StoreError::NotFound(_))
        ));
        assert!(matches!(
            store.add_allowed_tool(&missing, "Read").await,
            Err(StoreError::NotFound(_))
        ));
    }

    #[tokio::test]
    async fn bad_allowed_tools_json_reads_as_empty() {
        let store = Store::in_memory().await.unwrap();
        store.create_task(&task("PROJ-1")).await.unwrap();
        sqlx::query("UPDATE tasks SET allowed_tools = 'not json'")
            .execute(&store.pool)
            .await
            .unwrap();
        let t = store
            .get_task(&TaskKey::new("PROJ-1"))
            .await
            .unwrap()
            .unwrap();
        assert!(t.allowed_tools.is_empty());
    }

    #[tokio::test]
    async fn migration_0019_gives_defaults_to_existing_rows() {
        let options = SqliteConnectOptions::from_str("sqlite::memory:").unwrap();
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await
            .unwrap();
        let mut old = sqlx::migrate!("./migrations");
        old.migrations = old
            .migrations
            .iter()
            .filter(|m| m.version <= 18)
            .cloned()
            .collect::<Vec<_>>()
            .into();
        old.run(&pool).await.unwrap();
        sqlx::query(
            "INSERT INTO tasks (key, title, project, workflow, phase, status, created_at, updated_at)
             VALUES ('OLD-1', 'Old', NULL, 'feature', 'backlog', '{\"type\":\"queued\"}', 'x', 'x')",
        )
        .execute(&pool)
        .await
        .unwrap();

        let store = Store::connect(pool).await.unwrap();
        let t = store
            .get_task(&TaskKey::new("OLD-1"))
            .await
            .unwrap()
            .unwrap();
        assert!(!t.chat_mode);
        assert!(t.allowed_tools.is_empty());
    }

    #[tokio::test]
    async fn start_message_round_trips() {
        let store = Store::in_memory().await.unwrap();
        let key = TaskKey::new("PROJ-1");
        let mut t = task("PROJ-1");
        t.workflow = "prompt".into();
        t.start_message = Some("haz {ticket}".into());
        store.create_task(&t).await.unwrap();
        let got = store.get_task(&key).await.unwrap().unwrap();
        assert_eq!(got.start_message.as_deref(), Some("haz {ticket}"));
        assert_eq!(
            store.list_tasks().await.unwrap()[0]
                .start_message
                .as_deref(),
            Some("haz {ticket}")
        );
        store.set_start_message(&key, Some("otro")).await.unwrap();
        let got = store.get_task(&key).await.unwrap().unwrap();
        assert_eq!(got.start_message.as_deref(), Some("otro"));
        store.set_start_message(&key, None).await.unwrap();
        let got = store.get_task(&key).await.unwrap().unwrap();
        assert_eq!(got.start_message, None);
        assert!(
            store
                .set_start_message(&TaskKey::new("NOPE-1"), None)
                .await
                .is_err()
        );
    }

    /// The migration's UPDATEs (everything but the ALTER), to replay them.
    fn workflow_updates() -> Vec<&'static str> {
        include_str!("../migrations/0022_workflow_commands.sql")
            .split(';')
            .map(str::trim)
            .filter(|s| s.contains("UPDATE"))
            .collect()
    }

    async fn migrated_0022(language: Option<&str>) -> SqlitePool {
        let options = SqliteConnectOptions::from_str("sqlite::memory:").unwrap();
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await
            .unwrap();
        let mut old = sqlx::migrate!("./migrations");
        old.migrations = old
            .migrations
            .iter()
            .filter(|m| m.version <= 21)
            .cloned()
            .collect::<Vec<_>>()
            .into();
        old.run(&pool).await.unwrap();
        for (name, settings) in [
            ("empty", "{}"),
            ("tmpl", r#"{"branch_template":"x/{key}"}"#),
            (
                "done",
                r#"{"workflow":{"kind":"single","start":{"type":"nothing"}}}"#,
            ),
        ] {
            sqlx::query(
                "INSERT INTO projects (name, path, kind, workflow, created_at, settings)
                 VALUES (?, ?, 'repo', 'feature', 'x', ?)",
            )
            .bind(name)
            .bind(format!("/tmp/{name}"))
            .bind(settings)
            .execute(&pool)
            .await
            .unwrap();
        }
        for (key, wf) in [("T-1", "feature"), ("T-2", "prompt"), ("T-3", "bugfix")] {
            sqlx::query(
                "INSERT INTO tasks (key, title, project, workflow, phase, status, created_at, updated_at)
                 VALUES (?, 'T', NULL, ?, 'backlog', '{\"type\":\"queued\"}', 'x', 'x')",
            )
            .bind(key)
            .bind(wf)
            .execute(&pool)
            .await
            .unwrap();
        }
        if let Some(l) = language {
            sqlx::query("INSERT INTO settings (key, value) VALUES ('pref:language', ?)")
                .bind(serde_json::to_string(l).unwrap())
                .execute(&pool)
                .await
                .unwrap();
        }
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        pool
    }

    async fn project_settings(pool: &SqlitePool, name: &str) -> serde_json::Value {
        let s: String = sqlx::query_scalar("SELECT settings FROM projects WHERE name = ?")
            .bind(name)
            .fetch_one(pool)
            .await
            .unwrap();
        serde_json::from_str(&s).unwrap()
    }

    #[tokio::test]
    async fn migration_0022_gives_old_projects_the_feature_preset() {
        use crate::lang::{Lang, feature_preset};
        for (language, lang) in [
            (None, Lang::Es),
            (Some("en"), Lang::En),
            (Some("es"), Lang::Es),
        ] {
            let pool = migrated_0022(language).await;
            let want = serde_json::to_value(feature_preset(lang)).unwrap();
            let empty = project_settings(&pool, "empty").await;
            assert_eq!(empty["workflow"], want, "{language:?}");
            let tmpl = project_settings(&pool, "tmpl").await;
            assert_eq!(tmpl["workflow"], want);
            assert_eq!(tmpl["branch_template"], "x/{key}");
            // A project that already has a workflow keeps it.
            let done = project_settings(&pool, "done").await;
            assert_eq!(done["workflow"]["kind"], "single");

            let wfs: Vec<(String, Option<String>)> =
                sqlx::query_as("SELECT workflow, start_message FROM tasks ORDER BY key")
                    .fetch_all(&pool)
                    .await
                    .unwrap();
            assert_eq!(
                wfs,
                [
                    ("project".to_string(), None),
                    ("prompt".to_string(), None),
                    ("prompt".to_string(), None)
                ]
            );

            // Replaying the UPDATEs changes nothing (the ALTER cannot run twice).
            for sql in workflow_updates() {
                sqlx::query(sql).execute(&pool).await.unwrap();
            }
            assert_eq!(project_settings(&pool, "empty").await["workflow"], want);
            assert_eq!(project_settings(&pool, "tmpl").await["workflow"], want);
        }
    }

    #[tokio::test]
    async fn migrations_0020_and_0021_keep_existing_tasks_and_sessions() {
        let options = SqliteConnectOptions::from_str("sqlite::memory:").unwrap();
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await
            .unwrap();
        let mut old = sqlx::migrate!("./migrations");
        old.migrations = old
            .migrations
            .iter()
            .filter(|m| m.version <= 19)
            .cloned()
            .collect::<Vec<_>>()
            .into();
        old.run(&pool).await.unwrap();
        sqlx::query(
            "INSERT INTO tasks (key, title, project, workflow, phase, status, created_at, updated_at)
             VALUES ('OLD-1', 'Old', NULL, 'feature', 'backlog', '{\"type\":\"queued\"}', 'x', 'x')",
        )
        .execute(&pool)
        .await
        .unwrap();
        let cols: Vec<String> =
            sqlx::query_scalar("SELECT name FROM pragma_table_info('sessions')")
                .fetch_all(&pool)
                .await
                .unwrap();
        assert!(!cols.contains(&"runtime".to_string()));
        sqlx::query(
            "INSERT INTO sessions (id, task_key, kind, label, is_primary, created_at)
             VALUES ('s1', 'OLD-1', 'claude', 'Main', 1, 'x')",
        )
        .execute(&pool)
        .await
        .unwrap();

        let store = Store::connect(pool).await.unwrap();
        assert!(
            store
                .get_task(&TaskKey::new("OLD-1"))
                .await
                .unwrap()
                .is_some()
        );
        let session = store.session("s1").await.unwrap().unwrap();
        assert_eq!(session.label, "Main");
        store
            .dismiss(&[("a".into(), "OLD-1".into())])
            .await
            .unwrap();
        assert_eq!(store.dismissed().await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn deleting_a_task_cascades_its_dismissals() {
        let store = Store::in_memory().await.unwrap();
        store.create_task(&task("PROJ-1")).await.unwrap();
        store.create_task(&task("PROJ-2")).await.unwrap();
        store
            .dismiss(&[("a".into(), "PROJ-1".into()), ("b".into(), "PROJ-2".into())])
            .await
            .unwrap();
        store.delete_task(&TaskKey::new("PROJ-1")).await.unwrap();
        let left = store.dismissed().await.unwrap();
        assert_eq!(left, std::collections::HashSet::from(["b".to_string()]));
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
    async fn title_source_branch_error_and_previous_title_persist() {
        let store = Store::in_memory().await.unwrap();
        let key = TaskKey::new("PROJ-1");
        let mut t = task("PROJ-1");
        t.title_source = TitleSource::Initial;
        t.branch_error = Some("no".into());
        store.create_task(&t).await.unwrap();
        assert_eq!(store.get_task(&key).await.unwrap(), Some(t));

        store
            .set_title_source(&key, TitleSource::Spec, Some("Card retries"))
            .await
            .unwrap();
        store.set_branch_error(&key, None).await.unwrap();
        let got = store.get_task(&key).await.unwrap().unwrap();
        assert_eq!(got.title_source, TitleSource::Spec);
        assert_eq!(got.previous_title.as_deref(), Some("Card retries"));
        assert_eq!(got.branch_error, None);
        store.set_branch_error(&key, Some("boom")).await.unwrap();
        assert_eq!(
            store.list_tasks().await.unwrap()[0].branch_error.as_deref(),
            Some("boom")
        );
        let missing = TaskKey::new("NOPE-1");
        assert!(matches!(
            store.set_branch_error(&missing, None).await,
            Err(StoreError::NotFound(_))
        ));
        assert!(matches!(
            store
                .set_title_source(&missing, TitleSource::Manual, None)
                .await,
            Err(StoreError::NotFound(_))
        ));
    }

    #[tokio::test]
    async fn a_row_from_before_the_migration_reads_manual() {
        let store = Store::in_memory().await.unwrap();
        sqlx::query(
            "INSERT INTO tasks (key, title, workflow, status, created_at, updated_at)
             VALUES ('OLD-1', 't', 'feature', '{\"type\":\"queued\"}', 'x', 'x')",
        )
        .execute(&store.pool)
        .await
        .unwrap();
        let got = store
            .get_task(&TaskKey::new("OLD-1"))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(got.title_source, TitleSource::Manual);
        assert_eq!((got.previous_title, got.branch_error), (None, None));
    }

    #[tokio::test]
    async fn set_title_makes_it_manual_and_refuses_archived_tasks() {
        use delilah_core::{Event, ProjectKind};
        let store = Store::in_memory().await.unwrap();
        let key = TaskKey::new("PROJ-1");
        store.create_task(&task("PROJ-1")).await.unwrap();
        store
            .set_title_source(&key, TitleSource::Spec, Some("old"))
            .await
            .unwrap();
        for e in [Event::Start, Event::WorkspaceReady] {
            store.apply_event(&key, e, Utc::now()).await.unwrap();
        }
        let ws = delilah_git::Workspace {
            path: "/w".into(),
            branch: String::new(),
            kind: ProjectKind::Repo,
            submodules: vec![],
        };
        store
            .save_workspace(&key, "/p".as_ref(), &ws)
            .await
            .unwrap();

        let got = store.set_title(&key, "Mine").await.unwrap();
        assert_eq!(got.title, "Mine");
        assert_eq!(got.title_source, TitleSource::Manual);
        assert_eq!(got.previous_title, None);

        store
            .apply_event(&key, Event::MarkDone, Utc::now())
            .await
            .unwrap();
        store
            .set_title(&key, "Done but not archived")
            .await
            .unwrap();
        store.delete_workspace(&key).await.unwrap();
        let err = store.set_title(&key, "Late").await.unwrap_err();
        assert!(matches!(err, StoreError::Transition(_)), "{err}");
        assert_eq!(
            store.get_task(&key).await.unwrap().unwrap().title,
            "Done but not archived"
        );
        let err = store
            .set_title(&TaskKey::new("X-1"), "t")
            .await
            .unwrap_err();
        assert!(matches!(err, StoreError::NotFound(_)), "{err}");
    }

    #[tokio::test]
    async fn changing_a_backlog_title_makes_it_manual() {
        let store = Store::in_memory().await.unwrap();
        let key = TaskKey::new("PROJ-1");
        let mut t = task("PROJ-1");
        t.title_source = TitleSource::Initial;
        store.create_task(&t).await.unwrap();
        let notes_only = BacklogChange {
            notes: Some("n".into()),
            ..Default::default()
        };
        let got = store.update_backlog_task(&key, &notes_only).await.unwrap();
        assert_eq!(got.title_source, TitleSource::Initial);
        let retitle = BacklogChange {
            title: Some("New".into()),
            ..Default::default()
        };
        let got = store.update_backlog_task(&key, &retitle).await.unwrap();
        assert_eq!(got.title_source, TitleSource::Manual);
    }

    #[tokio::test]
    async fn rename_from_spec_happens_once_and_only_from_initial() {
        let store = Store::in_memory().await.unwrap();
        let key = TaskKey::new("LIBRE-1");
        let mut t = task("LIBRE-1");
        t.title_source = TitleSource::Initial;
        store.create_task(&t).await.unwrap();
        let got = store
            .rename_from_spec(&key, "Cards")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(got.title, "Cards");
        assert_eq!(got.title_source, TitleSource::Spec);
        assert_eq!(got.previous_title.as_deref(), Some(t.title.as_str()));
        assert_eq!(store.rename_from_spec(&key, "Other").await.unwrap(), None);
        assert_eq!(store.get_task(&key).await.unwrap().unwrap().title, "Cards");
    }

    #[tokio::test]
    async fn saving_mode_and_handoff_persist() {
        use delilah_core::{HandoffStage, Step};

        let store = Store::in_memory().await.unwrap();
        let key = TaskKey::new("PROJ-1");
        let mut saving = task("PROJ-1");
        saving.saving_mode = true;
        store.create_task(&saving).await.unwrap();
        assert_eq!(store.get_task(&key).await.unwrap(), Some(saving));
        store.create_task(&task("PROJ-2")).await.unwrap();
        let plain = store
            .get_task(&TaskKey::new("PROJ-2"))
            .await
            .unwrap()
            .unwrap();
        assert!(!plain.saving_mode && plain.handoff.is_none());

        store.set_saving_mode(&key, false).await.unwrap();
        assert!(!store.get_task(&key).await.unwrap().unwrap().saving_mode);
        store.set_saving_mode(&key, true).await.unwrap();
        assert!(store.get_task(&key).await.unwrap().unwrap().saving_mode);

        let handoff = Handoff {
            to: Step::Implement,
            stage: HandoffStage::Offered,
            error: None,
        };
        assert_eq!(
            serde_json::to_string(&handoff).unwrap(),
            r#"{"to":"implement","stage":"offered"}"#
        );
        store.set_handoff(&key, Some(&handoff)).await.unwrap();
        assert_eq!(
            store.get_task(&key).await.unwrap().unwrap().handoff,
            Some(handoff)
        );
        store.set_handoff(&key, None).await.unwrap();
        assert_eq!(store.get_task(&key).await.unwrap().unwrap().handoff, None);

        let missing = TaskKey::new("PROJ-404");
        assert!(matches!(
            store.set_saving_mode(&missing, true).await,
            Err(StoreError::NotFound(_))
        ));
    }

    #[tokio::test]
    async fn step_started_is_listed_in_the_history_and_keeps_the_status() {
        let store = Store::in_memory().await.unwrap();
        let key = TaskKey::new("PROJ-1");
        store.create_task(&task("PROJ-1")).await.unwrap();
        store
            .apply_event(&key, Event::Start, Utc::now())
            .await
            .unwrap();
        let event = Event::StepStarted {
            step: delilah_core::Step::Implement,
            session: "s-2".into(),
        };
        let task = store
            .apply_event(&key, event.clone(), Utc::now())
            .await
            .unwrap();
        assert_eq!(task.status, TaskStatus::PreparingWorkspace);
        let last = store.events(&key).await.unwrap().pop().unwrap();
        assert_eq!(last.event, event);
    }

    fn autopilot() -> Autopilot {
        Autopilot {
            state: delilah_core::AutopilotState::Running,
            stage: None,
            reason: None,
            since: Utc::now(),
            nudged: None,
            review_session: None,
        }
    }

    #[tokio::test]
    async fn update_autopilot_is_a_conditional_write() {
        let store = Store::in_memory().await.unwrap();
        let key = TaskKey::new("PROJ-1");
        store.create_task(&task("PROJ-1")).await.unwrap();
        // Nothing stored and nothing to write.
        assert!(!store.update_autopilot(&key, |_| None).await.unwrap());
        // The closure sees the stored value and its answer is written.
        assert!(
            store
                .update_autopilot(&key, |cur| {
                    assert!(cur.is_none());
                    Some(Some(autopilot()))
                })
                .await
                .unwrap()
        );
        let t = store.get_task(&key).await.unwrap().unwrap();
        assert!(t.autopilot.is_some() && t.saving_mode);
        assert!(
            store
                .update_autopilot(&key, |cur| cur.map(|mut a| {
                    a.nudged = Some(delilah_core::Step::Plan);
                    Some(a)
                }))
                .await
                .unwrap()
        );
        assert_eq!(
            store
                .get_task(&key)
                .await
                .unwrap()
                .unwrap()
                .autopilot
                .unwrap()
                .nudged,
            Some(delilah_core::Step::Plan)
        );
        // Cleared, a later update sees `None`.
        assert!(store.update_autopilot(&key, |_| Some(None)).await.unwrap());
        assert!(
            !store
                .update_autopilot(&key, |cur| cur.map(Some))
                .await
                .unwrap()
        );
        assert!(matches!(
            store
                .update_autopilot(&TaskKey::new("NOPE-1"), |_| None)
                .await,
            Err(StoreError::NotFound(_))
        ));
    }

    #[tokio::test]
    async fn autopilot_is_stored_with_the_task_and_implies_saving_mode() {
        let store = Store::in_memory().await.unwrap();
        let key = TaskKey::new("PROJ-1");
        let mut created = task("PROJ-1");
        created.autopilot = Some(autopilot());
        store.create_task(&created).await.unwrap();
        assert_eq!(store.get_task(&key).await.unwrap(), Some(created));

        store.create_task(&task("PROJ-2")).await.unwrap();
        let key2 = TaskKey::new("PROJ-2");
        let a = autopilot();
        store.set_autopilot(&key2, Some(&a)).await.unwrap();
        let t = store.get_task(&key2).await.unwrap().unwrap();
        assert_eq!(t.autopilot, Some(a));
        assert!(t.saving_mode);
        assert_eq!(store.list_tasks().await.unwrap()[1].autopilot, t.autopilot);

        store.set_autopilot(&key2, None).await.unwrap();
        let t = store.get_task(&key2).await.unwrap().unwrap();
        assert_eq!(t.autopilot, None);
        assert!(t.saving_mode);

        store
            .set_autopilot(&key2, Some(&autopilot()))
            .await
            .unwrap();
        store.set_saving_mode(&key2, true).await.unwrap();
        assert!(
            store
                .get_task(&key2)
                .await
                .unwrap()
                .unwrap()
                .autopilot
                .is_some()
        );
        store.set_saving_mode(&key2, false).await.unwrap();
        let t = store.get_task(&key2).await.unwrap().unwrap();
        assert_eq!(t.autopilot, None);
        assert!(!t.saving_mode);

        assert!(matches!(
            store.set_autopilot(&TaskKey::new("NOPE-1"), None).await,
            Err(StoreError::NotFound(_))
        ));
    }

    #[tokio::test]
    async fn autopilot_events_appear_in_the_history() {
        let store = Store::in_memory().await.unwrap();
        let key = TaskKey::new("PROJ-1");
        store.create_task(&task("PROJ-1")).await.unwrap();
        let events = vec![
            Event::AutopilotPaused {
                reason: "tests fail".into(),
            },
            Event::AutopilotResumed,
            Event::AutopilotAnswered {
                question: "q".into(),
                answer: "a".into(),
            },
            Event::AutopilotNudged {
                step: delilah_core::Step::Plan,
            },
            Event::AutopilotFinished {
                prs: vec!["https://x/pr/1".into()],
            },
        ];
        for e in &events {
            store
                .apply_event(&key, e.clone(), Utc::now())
                .await
                .unwrap();
        }
        let history: Vec<_> = store
            .events(&key)
            .await
            .unwrap()
            .into_iter()
            .map(|r| r.event)
            .collect();
        assert_eq!(history, events);
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
    async fn a_task_reports_the_branch_of_its_workspace() {
        let store = Store::in_memory().await.unwrap();
        let key = TaskKey::new("PROJ-1");
        store.create_task(&task("PROJ-1")).await.unwrap();
        assert_eq!(store.get_task(&key).await.unwrap().unwrap().branch, None);
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
        let want = Some("feature/PROJ-1".to_string());
        assert_eq!(store.get_task(&key).await.unwrap().unwrap().branch, want);
        assert_eq!(store.list_tasks().await.unwrap()[0].branch, want);
        store.delete_workspace(&key).await.unwrap();
        assert_eq!(store.get_task(&key).await.unwrap().unwrap().branch, None);
        assert_eq!(store.list_tasks().await.unwrap()[0].branch, None);
        let json = serde_json::to_value(task("PROJ-2")).unwrap();
        assert!(json.get("branch").is_none());
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
                conversation: false,
                step: None,
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
                workflow: "project".into(),
                settings: Default::default(),
                preview_command: None,
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

    #[tokio::test]
    async fn creates_a_task_from_a_conversation_in_one_step() {
        let store = Store::in_memory().await.unwrap();
        let at = Utc::now();
        store
            .create_task_from_conversation(&task("PROJ-1"), "s-1", "Idea", at)
            .await
            .unwrap();
        let key = TaskKey::new("PROJ-1");
        assert!(store.get_task(&key).await.unwrap().is_some());
        let events = store.events(&key).await.unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(
            events[0].event,
            Event::CreatedFromConversation {
                session: "s-1".into(),
                label: "Idea".into()
            }
        );
        let json: String = sqlx::query_scalar("SELECT event FROM task_events WHERE task_key = ?")
            .bind("PROJ-1")
            .fetch_one(&store.pool)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(
            json,
            serde_json::json!({"type": "created-from-conversation", "session": "s-1", "label": "Idea"})
        );

        let err = store
            .create_task_from_conversation(&task("PROJ-1"), "s-2", "Otra", at)
            .await
            .unwrap_err();
        assert!(matches!(err, StoreError::AlreadyExists(_)), "{err}");
        assert_eq!(store.events(&key).await.unwrap().len(), 1, "no stray event");
    }
}
