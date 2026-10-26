//! The sessions opened in each task's workspace.

use chrono::{DateTime, Utc};
use delilah_agents::runtime::RuntimeKind;
use delilah_core::{Event, Step, TaskKey};
use serde::{Deserialize, Serialize};
use serde_json::Value;
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
    /// An idea conversation: a project's Claude session shown as a chat.
    pub conversation: bool,
    /// The step of the saving-mode chain it runs; `None` for any other session.
    pub step: Option<Step>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum EntryKind {
    User,
    Claude,
    Card,
    /// A line the daemon adds to the conversation, e.g. `{"marker": "stopped"}`.
    Marker,
}

impl EntryKind {
    pub fn name(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Claude => "claude",
            Self::Card => "card",
            Self::Marker => "marker",
        }
    }

    fn parse(s: &str) -> Option<Self> {
        match s {
            "user" => Some(Self::User),
            "claude" => Some(Self::Claude),
            "card" => Some(Self::Card),
            "marker" => Some(Self::Marker),
            _ => None,
        }
    }
}

/// One line of an idea conversation; the body's shape depends on the kind.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Entry {
    pub id: i64,
    pub at: DateTime<Utc>,
    pub kind: EntryKind,
    /// Always a JSON object; its fields sit next to `id`, `at` and `kind` when serialised.
    #[serde(flatten)]
    pub body: Value,
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
        let row = sqlx::query(&format!(
            "{SELECT} WHERE task_key = ? AND step = ? AND closed = 0"
        ))
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
        _from_id: &str,
        new: &SessionRecord,
    ) -> Result<SessionRecord, StoreError> {
        let step = new
            .step
            .ok_or_else(|| StoreError::Corrupt("a handoff session needs a step".into()))?;
        // IMMEDIATE, as in `apply_event`: a retry waits for the first attempt's result.
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let existing = sqlx::query(&format!(
            "{SELECT} WHERE task_key = ? AND step = ? AND closed = 0"
        ))
        .bind(task)
        .bind(step.name())
        .fetch_optional(&mut *tx)
        .await?;
        if let Some(row) = existing {
            return session_from_row(&row);
        }
        sqlx::query("UPDATE sessions SET is_primary = 0 WHERE task_key = ? AND is_primary = 1")
            .bind(task)
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
                step,
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
        sqlx::query("UPDATE sessions SET is_primary = 0 WHERE task_key = ? AND is_primary = 1")
            .bind(task)
            .execute(&mut *tx)
            .await?;
        sqlx::query("UPDATE sessions SET is_primary = 0, closed = 1 WHERE task_key = ? AND id = ?")
            .bind(task)
            .bind(new_id)
            .execute(&mut *tx)
            .await?;
        let promoted =
            sqlx::query("UPDATE sessions SET is_primary = 1 WHERE task_key = ? AND id = ?")
                .bind(task)
                .bind(from_id)
                .execute(&mut *tx)
                .await?;
        if promoted.rows_affected() == 0 {
            // Dropping `tx` rolls the demotions back.
            return Err(StoreError::NotFound(TaskKey::new(task)));
        }
        // The history must not list a step that was undone.
        sqlx::query(
            "DELETE FROM task_events WHERE task_key = ? AND json_extract(event, '$.type') = 'step-started'
             AND json_extract(event, '$.session') = ?",
        )
        .bind(task)
        .bind(new_id)
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

    /// Puts an adopted session back as the project's, as `original` was before
    /// [`adopt_session`](Self::adopt_session) (a start that did not get its agent).
    pub async fn unadopt_session(&self, original: &SessionRecord) -> Result<(), StoreError> {
        sqlx::query(
            "UPDATE sessions SET task_key = ?, project = ?, is_primary = ?, read_only = ?,
             label = ? WHERE id = ?",
        )
        .bind(
            project_of(&original.task)
                .is_none()
                .then_some(&original.task),
        )
        .bind(project_of(&original.task))
        .bind(original.primary)
        .bind(original.read_only)
        .bind(&original.label)
        .bind(&original.id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Deletes a session's record, as if it had never been created (a start that failed).
    pub async fn forget_session(&self, id: &str) -> Result<(), StoreError> {
        sqlx::query("DELETE FROM sessions WHERE id = ?")
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

/// What a session has beyond its record: its own runtime and its last activity.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SessionExtras {
    /// The stored per-session runtime; `None` follows the task's `chat_mode`.
    pub runtime_override: Option<RuntimeKind>,
    pub last_activity_at: Option<DateTime<Utc>>,
    /// Claude's last message, or the last prompt; at most [`MAX_LAST_MESSAGE`] chars.
    pub last_message: Option<String>,
}

pub const MAX_LAST_MESSAGE: usize = 200;

pub fn clip_message(s: &str) -> String {
    s.trim().chars().take(MAX_LAST_MESSAGE).collect()
}

fn runtime_word(kind: RuntimeKind) -> &'static str {
    match kind {
        RuntimeKind::Pty => "pty",
        RuntimeKind::Stream => "stream",
    }
}

impl Store {
    pub async fn session_extras(&self, id: &str) -> Result<SessionExtras, StoreError> {
        let row = sqlx::query(
            "SELECT runtime, last_activity_at, last_message FROM sessions WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        let Some(row) = row else {
            return Ok(SessionExtras::default());
        };
        let runtime: Option<String> = row.try_get("runtime")?;
        let at: Option<String> = row.try_get("last_activity_at")?;
        Ok(SessionExtras {
            runtime_override: match runtime.as_deref() {
                Some("pty") => Some(RuntimeKind::Pty),
                Some("stream") => Some(RuntimeKind::Stream),
                _ => None,
            },
            last_activity_at: at
                .and_then(|a| DateTime::parse_from_rfc3339(&a).ok())
                .map(|a| a.with_timezone(&Utc)),
            last_message: row.try_get("last_message")?,
        })
    }

    /// Stores the session's own runtime, or `None` to follow the task again.
    pub async fn set_runtime_override(
        &self,
        id: &str,
        runtime: Option<RuntimeKind>,
    ) -> Result<(), StoreError> {
        sqlx::query("UPDATE sessions SET runtime = ? WHERE id = ?")
            .bind(runtime.map(runtime_word))
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Follows the task again: every session of it loses its own runtime.
    pub async fn clear_runtime_overrides(&self, task: &str) -> Result<(), StoreError> {
        sqlx::query("UPDATE sessions SET runtime = NULL WHERE task_key = ?")
            .bind(task)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Notes a turn event: now is its last activity; `message` (clipped) replaces the
    /// last message when there is one.
    pub async fn touch_session(&self, id: &str, message: Option<&str>) -> Result<(), StoreError> {
        let message = message.map(clip_message).filter(|m| !m.is_empty());
        sqlx::query(
            "UPDATE sessions SET last_activity_at = ?, last_message = COALESCE(?, last_message) WHERE id = ?",
        )
        .bind(Utc::now().to_rfc3339())
        .bind(message)
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
        "INSERT INTO sessions (id, task_key, project, kind, label, is_primary, closed, created_at, read_only, step, conversation)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
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
    .bind(s.conversation)
    .execute(executor)
    .await?;
    Ok(())
}

const SELECT: &str = "SELECT id, task_key, project, kind, label, is_primary, closed, read_only, step, conversation FROM sessions";

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
            "SELECT id, task_key, project, kind, label, is_primary, closed, read_only, step, conversation, attention FROM sessions
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

impl Store {
    /// Appends a line to a conversation.
    pub async fn add_entry(
        &self,
        session: &str,
        kind: EntryKind,
        body: &Value,
    ) -> Result<Entry, StoreError> {
        let at = Utc::now();
        let id = sqlx::query(
            "INSERT INTO conversation_entries (session_id, at, kind, body) VALUES (?, ?, ?, ?)",
        )
        .bind(session)
        .bind(at.to_rfc3339())
        .bind(kind.name())
        .bind(serde_json::to_string(body)?)
        .execute(&self.pool)
        .await?
        .last_insert_rowid();
        Ok(Entry {
            id,
            at,
            kind,
            body: body.clone(),
        })
    }

    /// A conversation's whole history, in the order it happened.
    pub async fn entries(&self, session: &str) -> Result<Vec<Entry>, StoreError> {
        let rows = sqlx::query(
            "SELECT id, at, kind, body FROM conversation_entries WHERE session_id = ? ORDER BY id",
        )
        .bind(session)
        .fetch_all(&self.pool)
        .await?;
        rows.iter().map(entry_from_row).collect()
    }

    /// The user's messages that wait for the turn to end, oldest first.
    pub async fn queued_entries(&self, session: &str) -> Result<Vec<Entry>, StoreError> {
        Ok(self
            .entries(session)
            .await?
            .into_iter()
            .filter(is_queued)
            .collect())
    }

    /// Takes every queued user message of a session in one statement, marking it
    /// `sent`, oldest first. Whoever gets an entry here is the only one to type it.
    pub async fn claim_queued(&self, session: &str) -> Result<Vec<Entry>, StoreError> {
        let rows = sqlx::query(
            "UPDATE conversation_entries SET body = json_set(body, '$.state', 'sent')
             WHERE session_id = ? AND kind = 'user' AND json_extract(body, '$.state') = 'queued'
             RETURNING id, at, kind, body",
        )
        .bind(session)
        .fetch_all(&self.pool)
        .await?;
        let mut entries = rows
            .iter()
            .map(entry_from_row)
            .collect::<Result<Vec<_>, _>>()?;
        entries.sort_by_key(|e| e.id);
        Ok(entries)
    }

    /// Puts messages claimed by `claim_queued` back in the queue (typing them failed).
    pub async fn requeue_entries(&self, session: &str, ids: &[i64]) -> Result<(), StoreError> {
        for id in ids {
            sqlx::query(
                "UPDATE conversation_entries SET body = json_set(body, '$.state', 'queued')
                 WHERE id = ? AND session_id = ? AND kind = 'user'
                 AND json_extract(body, '$.state') = 'sent'",
            )
            .bind(id)
            .bind(session)
            .execute(&self.pool)
            .await?;
        }
        Ok(())
    }

    /// Deletes a user message recorded `sent` whose typing failed.
    pub async fn discard_sent_entry(&self, session: &str, id: i64) -> Result<(), StoreError> {
        sqlx::query(
            "DELETE FROM conversation_entries WHERE id = ? AND session_id = ? AND kind = 'user'
             AND json_extract(body, '$.state') = 'sent'",
        )
        .bind(id)
        .bind(session)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Marks every card still `pending` as `withdrawn` (the asks live in memory only,
    /// so after a restart none of them can be answered); returns how many.
    pub async fn withdraw_pending_cards(&self) -> Result<u64, StoreError> {
        let done = sqlx::query(
            "UPDATE conversation_entries SET body = json_set(body, '$.outcome', 'withdrawn')
             WHERE kind = 'card' AND json_extract(body, '$.outcome') = 'pending'",
        )
        .execute(&self.pool)
        .await?;
        Ok(done.rows_affected())
    }

    /// The card of an ask in a conversation's history, if it is there.
    pub async fn card_entry(&self, session: &str, ask: &str) -> Result<Option<Entry>, StoreError> {
        let row = sqlx::query(
            "SELECT id, at, kind, body FROM conversation_entries
             WHERE session_id = ? AND kind = 'card' AND json_extract(body, '$.ask') = ?",
        )
        .bind(session)
        .bind(ask)
        .fetch_optional(&self.pool)
        .await?;
        row.as_ref().map(entry_from_row).transpose()
    }

    pub async fn update_entry(&self, id: i64, body: &Value) -> Result<(), StoreError> {
        sqlx::query("UPDATE conversation_entries SET body = ? WHERE id = ?")
            .bind(serde_json::to_string(body)?)
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Deletes a user message that is still queued; false when it is anything else.
    pub async fn delete_queued_entry(&self, session: &str, id: i64) -> Result<bool, StoreError> {
        let deleted = sqlx::query(
            "DELETE FROM conversation_entries WHERE id = ? AND session_id = ? AND kind = 'user'
             AND json_extract(body, '$.state') = 'queued'",
        )
        .bind(id)
        .bind(session)
        .execute(&self.pool)
        .await?;
        Ok(deleted.rows_affected() > 0)
    }
}

fn is_queued(e: &Entry) -> bool {
    e.kind == EntryKind::User && e.body["state"] == "queued"
}

fn entry_from_row(row: &SqliteRow) -> Result<Entry, StoreError> {
    let kind: String = row.try_get("kind")?;
    let at: String = row.try_get("at")?;
    let body: String = row.try_get("body")?;
    let body: Value = serde_json::from_str(&body)?;
    // Markers were first stored as `claude` entries with a `marker` field.
    let kind = if kind == "claude" && body.get("marker").is_some() {
        "marker".to_string()
    } else {
        kind
    };
    Ok(Entry {
        id: row.try_get("id")?,
        at: DateTime::parse_from_rfc3339(&at)
            .map_err(|e| sqlx::Error::Decode(Box::new(e)))?
            .with_timezone(&Utc),
        kind: EntryKind::parse(&kind)
            .ok_or_else(|| StoreError::Corrupt(format!("unknown entry kind `{kind}`")))?,
        body,
    })
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
        conversation: row.try_get("conversation")?,
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
    use delilah_core::Task;

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
            conversation: false,
            step: None,
        }
    }

    #[tokio::test]
    async fn the_activity_and_runtime_columns_round_trip() {
        let store = Store::in_memory().await.unwrap();
        store
            .create_task(&Task::new(TaskKey::new("PROJ-1"), "t", "feature"))
            .await
            .unwrap();
        store
            .create_session(&session("a", SessionKind::Claude, true))
            .await
            .unwrap();
        assert_eq!(
            store.session_extras("a").await.unwrap(),
            SessionExtras::default()
        );

        store
            .set_runtime_override("a", Some(RuntimeKind::Stream))
            .await
            .unwrap();
        store
            .touch_session("a", Some(&"x".repeat(300)))
            .await
            .unwrap();
        let extras = store.session_extras("a").await.unwrap();
        assert_eq!(extras.runtime_override, Some(RuntimeKind::Stream));
        assert!(extras.last_activity_at.is_some());
        assert_eq!(extras.last_message.unwrap().chars().count(), 200);

        // A touch without a message keeps the last one.
        store.touch_session("a", None).await.unwrap();
        assert!(
            store
                .session_extras("a")
                .await
                .unwrap()
                .last_message
                .is_some()
        );

        store.clear_runtime_overrides("PROJ-1").await.unwrap();
        assert_eq!(
            store.session_extras("a").await.unwrap().runtime_override,
            None
        );
    }

    #[test]
    fn entries_serialise_flat() {
        let at = DateTime::parse_from_rfc3339("2026-10-20T10:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let user = Entry {
            id: 1,
            at,
            kind: EntryKind::User,
            body: serde_json::json!({ "text": "hi", "state": "sent", "from": "view" }),
        };
        assert_eq!(
            serde_json::to_value(&user).unwrap(),
            serde_json::json!({
                "id": 1, "at": "2026-10-20T10:00:00Z", "kind": "user",
                "text": "hi", "state": "sent", "from": "view"
            })
        );
        let card = Entry {
            id: 2,
            at,
            kind: EntryKind::Card,
            body: serde_json::json!({ "card": "ask", "outcome": null }),
        };
        assert_eq!(
            serde_json::to_value(&card).unwrap(),
            serde_json::json!({
                "id": 2, "at": "2026-10-20T10:00:00Z", "kind": "card",
                "card": "ask", "outcome": null
            })
        );
    }

    #[tokio::test]
    async fn a_forgotten_session_is_gone() {
        let store = Store::in_memory().await.unwrap();
        store
            .create_task(&Task::new(TaskKey::new("PROJ-1"), "t", "feature"))
            .await
            .unwrap();
        store
            .create_session(&session("a", SessionKind::Claude, true))
            .await
            .unwrap();
        store.forget_session("a").await.unwrap();
        assert!(store.session("a").await.unwrap().is_none());
        assert!(store.primary_session("PROJ-1").await.unwrap().is_none());
        // Forgetting what is not there is not an error.
        store.forget_session("a").await.unwrap();
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
                step: Step::Implement,
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

    async fn count(store: &Store, sql: &str) -> i64 {
        sqlx::query_scalar(sql)
            .fetch_one(&store.pool)
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn a_handover_after_an_undo_starts_a_new_session() {
        let store = store_with_plan_session().await;
        store
            .hand_over("PROJ-1", "plan", &step_session("impl", Step::Implement))
            .await
            .unwrap();
        store
            .undo_hand_over("PROJ-1", "plan", "impl")
            .await
            .unwrap();
        let key = TaskKey::new("PROJ-1");
        assert!(store.events(&key).await.unwrap().is_empty());
        assert!(
            store
                .session_of_step("PROJ-1", Step::Implement)
                .await
                .unwrap()
                .is_none()
        );

        let retry = store
            .hand_over("PROJ-1", "plan", &step_session("impl-2", Step::Implement))
            .await
            .unwrap();
        assert_eq!(retry.id, "impl-2");
        assert!(retry.primary && !retry.closed);
        assert!(!store.session("plan").await.unwrap().unwrap().primary);
        assert_eq!(
            store.primary_session("PROJ-1").await.unwrap().unwrap().id,
            "impl-2"
        );
        assert_eq!(store.events(&key).await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn a_handover_from_a_stale_session_still_leaves_one_primary() {
        let store = store_with_plan_session().await;
        store
            .create_session(&session("other", SessionKind::Shell, false))
            .await
            .unwrap();
        store
            .hand_over("PROJ-1", "other", &step_session("impl", Step::Implement))
            .await
            .unwrap();
        let sql = "SELECT COUNT(*) FROM sessions WHERE task_key = 'PROJ-1' AND is_primary = 1";
        assert_eq!(count(&store, sql).await, 1);
        assert_eq!(
            store.primary_session("PROJ-1").await.unwrap().unwrap().id,
            "impl"
        );
    }

    #[tokio::test]
    async fn undoing_towards_an_unknown_session_changes_nothing() {
        let store = store_with_plan_session().await;
        store
            .hand_over("PROJ-1", "plan", &step_session("impl", Step::Implement))
            .await
            .unwrap();
        let err = store.undo_hand_over("PROJ-1", "ghost", "impl").await;
        assert!(matches!(err, Err(StoreError::NotFound(_))));
        assert_eq!(
            store.primary_session("PROJ-1").await.unwrap().unwrap().id,
            "impl"
        );
        assert!(!store.session("impl").await.unwrap().unwrap().closed);
        assert_eq!(
            store.events(&TaskKey::new("PROJ-1")).await.unwrap().len(),
            1
        );
    }

    #[tokio::test]
    async fn two_open_sessions_of_one_step_are_refused() {
        let store = store_with_plan_session().await;
        store
            .create_session(&step_session("impl", Step::Implement))
            .await
            .unwrap();
        let dup = store
            .create_session(&step_session("impl-2", Step::Implement))
            .await;
        assert!(dup.is_err());
    }

    fn conversation(id: &str) -> SessionRecord {
        SessionRecord {
            task: project_owner("web"),
            conversation: true,
            ..session(id, SessionKind::Claude, false)
        }
    }

    async fn with_project(store: &Store) {
        store
            .create_project(&delilah_core::Project {
                name: "web".into(),
                path: "/w/web".into(),
                kind: delilah_core::ProjectKind::Repo,
                workflow: "feature".into(),
                settings: Default::default(),
                preview_command: None,
            })
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn conversation_flag_reads_back() {
        let store = Store::in_memory().await.unwrap();
        with_project(&store).await;
        store.create_session(&conversation("c")).await.unwrap();
        store
            .create_task(&Task::new(TaskKey::new("PROJ-1"), "t", "feature"))
            .await
            .unwrap();
        store
            .create_session(&session("a", SessionKind::Claude, true))
            .await
            .unwrap();
        assert!(store.session("c").await.unwrap().unwrap().conversation);
        assert!(!store.session("a").await.unwrap().unwrap().conversation);
        assert!(store.open_sessions(None).await.unwrap()[0].conversation);
    }

    #[tokio::test]
    async fn entries_keep_their_order_and_bodies() {
        let store = Store::in_memory().await.unwrap();
        with_project(&store).await;
        store.create_session(&conversation("c")).await.unwrap();
        let user = serde_json::json!({"text": "hola", "state": "queued", "from": "view"});
        let first = store.add_entry("c", EntryKind::User, &user).await.unwrap();
        store
            .add_entry("c", EntryKind::Claude, &serde_json::json!({"text": "ok"}))
            .await
            .unwrap();
        store
            .add_entry("c", EntryKind::Card, &serde_json::json!({"ask": "a1"}))
            .await
            .unwrap();

        let all = store.entries("c").await.unwrap();
        let kinds: Vec<_> = all.iter().map(|e| e.kind).collect();
        assert_eq!(kinds, [EntryKind::User, EntryKind::Claude, EntryKind::Card]);
        assert_eq!(all[0], first);
        assert_eq!(all[0].body, user);
        assert_eq!(store.queued_entries("c").await.unwrap().len(), 1);

        let sent = serde_json::json!({"text": "hola", "state": "sent", "from": "view"});
        store.update_entry(first.id, &sent).await.unwrap();
        assert_eq!(store.entries("c").await.unwrap()[0].body, sent);
        assert!(store.queued_entries("c").await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn markers_have_their_own_kind_and_old_ones_still_read() {
        let store = Store::in_memory().await.unwrap();
        with_project(&store).await;
        store.create_session(&conversation("c")).await.unwrap();
        let body = serde_json::json!({"marker": "stopped"});
        store
            .add_entry("c", EntryKind::Marker, &body)
            .await
            .unwrap();
        // Written before `Marker` existed: a claude entry with a marker field.
        store
            .add_entry("c", EntryKind::Claude, &body)
            .await
            .unwrap();
        let all = store.entries("c").await.unwrap();
        assert!(
            all.iter()
                .all(|e| e.kind == EntryKind::Marker && e.body == body)
        );
        assert_eq!(serde_json::to_value(&all[0]).unwrap()["kind"], "marker");
    }

    #[tokio::test]
    async fn only_queued_user_entries_are_deleted() {
        let store = Store::in_memory().await.unwrap();
        with_project(&store).await;
        store.create_session(&conversation("c")).await.unwrap();
        let queued = serde_json::json!({"text": "a", "state": "queued", "from": "view"});
        let sent = serde_json::json!({"text": "b", "state": "sent", "from": "view"});
        let q = store
            .add_entry("c", EntryKind::User, &queued)
            .await
            .unwrap();
        let s = store.add_entry("c", EntryKind::User, &sent).await.unwrap();
        let c = store
            .add_entry("c", EntryKind::Claude, &serde_json::json!({"text": "x"}))
            .await
            .unwrap();

        assert!(!store.delete_queued_entry("c", s.id).await.unwrap());
        assert!(!store.delete_queued_entry("c", c.id).await.unwrap());
        assert!(store.delete_queued_entry("c", q.id).await.unwrap());
        assert!(!store.delete_queued_entry("c", q.id).await.unwrap());
        assert_eq!(store.entries("c").await.unwrap().len(), 2);
    }

    #[tokio::test]
    async fn claiming_the_queue_marks_it_sent_once() {
        let store = Store::in_memory().await.unwrap();
        with_project(&store).await;
        store.create_session(&conversation("c")).await.unwrap();
        for text in ["a", "b"] {
            let body = serde_json::json!({"text": text, "state": "queued", "from": "view"});
            store.add_entry("c", EntryKind::User, &body).await.unwrap();
        }
        let sent = serde_json::json!({"text": "s", "state": "sent", "from": "view"});
        store.add_entry("c", EntryKind::User, &sent).await.unwrap();

        let claimed = store.claim_queued("c").await.unwrap();
        let texts: Vec<_> = claimed.iter().map(|e| e.body["text"].clone()).collect();
        assert_eq!(texts, ["a", "b"]);
        assert!(claimed.iter().all(|e| e.body["state"] == "sent"));
        assert!(store.claim_queued("c").await.unwrap().is_empty());
        assert!(!store.delete_queued_entry("c", claimed[0].id).await.unwrap());
        assert!(store.queued_entries("c").await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn the_start_up_sweep_withdraws_pending_cards_only() {
        let store = Store::in_memory().await.unwrap();
        with_project(&store).await;
        store.create_session(&conversation("c")).await.unwrap();
        for outcome in ["pending", "answered"] {
            let body = serde_json::json!({"ask": outcome, "outcome": outcome});
            store.add_entry("c", EntryKind::Card, &body).await.unwrap();
        }
        assert_eq!(store.withdraw_pending_cards().await.unwrap(), 1);
        let outcomes: Vec<_> = store
            .entries("c")
            .await
            .unwrap()
            .iter()
            .map(|e| e.body["outcome"].clone())
            .collect();
        assert_eq!(outcomes, ["withdrawn", "answered"]);
        assert_eq!(store.withdraw_pending_cards().await.unwrap(), 0);
    }

    #[tokio::test]
    async fn deleting_the_project_deletes_its_entries() {
        let store = Store::in_memory().await.unwrap();
        with_project(&store).await;
        store.create_session(&conversation("c")).await.unwrap();
        store
            .add_entry("c", EntryKind::Claude, &serde_json::json!({"text": "x"}))
            .await
            .unwrap();
        store.delete_project("web").await.unwrap();
        assert!(store.entries("c").await.unwrap().is_empty());
        let left: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM conversation_entries")
            .fetch_one(&store.pool)
            .await
            .unwrap();
        assert_eq!(left, 0);
    }
}
