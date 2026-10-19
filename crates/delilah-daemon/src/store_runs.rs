//! Where each task's workspace and agent session live, so they survive a restart.

use std::path::PathBuf;

use delilah_core::{ProjectKind, TaskKey};
use delilah_git::Workspace;
use sqlx::Row;

use crate::store::{Store, StoreError};

impl Store {
    pub async fn save_workspace(
        &self,
        key: &TaskKey,
        parent: &std::path::Path,
        ws: &Workspace,
    ) -> Result<(), StoreError> {
        sqlx::query(
            "INSERT OR REPLACE INTO workspaces (task_key, parent, path, branch, kind, submodules)
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(key.as_str())
        .bind(parent.to_string_lossy())
        .bind(ws.path.to_string_lossy())
        .bind(&ws.branch)
        .bind(ws.kind.name())
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
            "SELECT parent, path, branch, kind, submodules FROM workspaces WHERE task_key = ?",
        )
        .bind(key.as_str())
        .fetch_optional(&self.pool)
        .await?;
        let Some(row) = row else { return Ok(None) };
        let submodules: String = row.try_get("submodules")?;
        let kind: String = row.try_get("kind")?;
        let kind = ProjectKind::parse(&kind)
            .ok_or_else(|| StoreError::Corrupt(format!("unknown workspace kind `{kind}`")))?;
        Ok(Some((
            PathBuf::from(row.try_get::<String, _>("parent")?),
            Workspace {
                path: PathBuf::from(row.try_get::<String, _>("path")?),
                branch: row.try_get("branch")?,
                kind,
                submodules: serde_json::from_str(&submodules)?,
            },
        )))
    }

    pub async fn delete_workspace(&self, key: &TaskKey) -> Result<(), StoreError> {
        sqlx::query("DELETE FROM workspaces WHERE task_key = ?")
            .bind(key.as_str())
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use delilah_core::Task;

    use super::*;

    #[tokio::test]
    async fn keeps_the_workspace_of_a_task() {
        let store = Store::in_memory().await.unwrap();
        let key = TaskKey::new("PROJ-1");
        store
            .create_task(&Task::new(key.clone(), "t", "feature"))
            .await
            .unwrap();
        let ws = Workspace {
            path: "/tmp/ws/PROJ-1".into(),
            branch: "feature/PROJ-1".into(),
            kind: ProjectKind::Parent,
            submodules: vec!["svc-payments".into()],
        };
        store
            .save_workspace(&key, "/tmp/parent".as_ref(), &ws)
            .await
            .unwrap();

        assert_eq!(
            store.workspace(&key).await.unwrap(),
            Some(("/tmp/parent".into(), ws))
        );
    }
}
