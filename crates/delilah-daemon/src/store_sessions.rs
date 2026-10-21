//! The sessions opened in each task's workspace.

use chrono::{DateTime, Utc};
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
    /// An idea conversation: a project's Claude session shown as a chat.
    pub conversation: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum EntryKind {
    User,
    Claude,
    Card,
}

impl EntryKind {
    pub fn name(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Claude => "claude",
            Self::Card => "card",
        }
    }

    fn parse(s: &str) -> Option<Self> {
        match s {
            "user" => Some(Self::User),
            "claude" => Some(Self::Claude),
            "card" => Some(Self::Card),
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
        sqlx::query(
            "INSERT INTO sessions (id, task_key, project, kind, label, is_primary, closed, created_at, read_only, conversation)
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
        .bind(s.conversation)
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

const SELECT: &str = "SELECT id, task_key, project, kind, label, is_primary, closed, read_only, conversation FROM sessions";

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
            "SELECT id, task_key, project, kind, label, is_primary, closed, read_only, conversation, attention FROM sessions
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
    Ok(Entry {
        id: row.try_get("id")?,
        at: DateTime::parse_from_rfc3339(&at)
            .map_err(|e| sqlx::Error::Decode(Box::new(e)))?
            .with_timezone(&Utc),
        kind: EntryKind::parse(&kind)
            .ok_or_else(|| StoreError::Corrupt(format!("unknown entry kind `{kind}`")))?,
        body: serde_json::from_str(&body)?,
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
            conversation: false,
        }
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
