//! Where each task's workspace and agent session live, so they survive a restart.

use std::path::PathBuf;

use chrono::Utc;
use delilah_core::TaskKey;
use delilah_git::Workspace;
use sqlx::Row;

use crate::store::{Store, StoreError};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentSession {
    pub agent: String,
    pub session_id: String,
}

impl Store {
    pub async fn save_workspace(
        &self,
        key: &TaskKey,
        parent: &std::path::Path,
        ws: &Workspace,
    ) -> Result<(), StoreError> {
        sqlx::query(
            "INSERT OR REPLACE INTO workspaces (task_key, parent, path, branch, submodules)
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind(key.as_str())
        .bind(parent.to_string_lossy())
        .bind(ws.path.to_string_lossy())
        .bind(&ws.branch)
        .bind(serde_json::to_string(&ws.submodules)?)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// The workspace and the parent repo it was created from.
    pub async fn workspace(
        &self,
        key: &TaskKey,
    ) -> Result<Option<(PathBuf, Workspace)>, StoreError> {
        let row = sqlx::query(
            "SELECT parent, path, branch, submodules FROM workspaces WHERE task_key = ?",
        )
        .bind(key.as_str())
        .fetch_optional(&self.pool)
        .await?;
        let Some(row) = row else { return Ok(None) };
        let submodules: String = row.try_get("submodules")?;
        Ok(Some((
            PathBuf::from(row.try_get::<String, _>("parent")?),
            Workspace {
                path: PathBuf::from(row.try_get::<String, _>("path")?),
                branch: row.try_get("branch")?,
                submodules: serde_json::from_str(&submodules)?,
            },
        )))
    }

    pub async fn save_agent_session(
        &self,
        key: &TaskKey,
        session: &AgentSession,
    ) -> Result<(), StoreError> {
        sqlx::query(
            "INSERT OR REPLACE INTO agent_sessions (task_key, agent, session_id, started_at)
             VALUES (?, ?, ?, ?)",
        )
        .bind(key.as_str())
        .bind(&session.agent)
        .bind(&session.session_id)
        .bind(Utc::now().to_rfc3339())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn agent_session(&self, key: &TaskKey) -> Result<Option<AgentSession>, StoreError> {
        let row = sqlx::query("SELECT agent, session_id FROM agent_sessions WHERE task_key = ?")
            .bind(key.as_str())
            .fetch_optional(&self.pool)
            .await?;
        row.map(|row| {
            Ok(AgentSession {
                agent: row.try_get("agent")?,
                session_id: row.try_get("session_id")?,
            })
        })
        .transpose()
    }
}

#[cfg(test)]
mod tests {
    use delilah_core::Task;

    use super::*;

    #[tokio::test]
    async fn keeps_the_workspace_and_session_of_a_task() {
        let store = Store::in_memory().await.unwrap();
        let key = TaskKey::new("PROJ-1");
        store
            .create_task(&Task::new(key.clone(), "t", "feature"))
            .await
            .unwrap();
        let ws = Workspace {
            path: "/tmp/ws/PROJ-1".into(),
            branch: "feature/PROJ-1".into(),
            submodules: vec!["svc-payments".into()],
        };
        store
            .save_workspace(&key, "/tmp/parent".as_ref(), &ws)
            .await
            .unwrap();
        let session = AgentSession {
            agent: "claude".into(),
            session_id: "abc".into(),
        };
        store.save_agent_session(&key, &session).await.unwrap();

        assert_eq!(
            store.workspace(&key).await.unwrap(),
            Some(("/tmp/parent".into(), ws))
        );
        assert_eq!(store.agent_session(&key).await.unwrap(), Some(session));
        assert_eq!(
            store.agent_session(&TaskKey::new("PROJ-2")).await.unwrap(),
            None
        );
    }
}
