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

    /// Marks a workspace as the project's own checkout, with where it was before.
    pub async fn set_checkout_previous(
        &self,
        key: &TaskKey,
        previous: &[(String, String)],
    ) -> Result<(), StoreError> {
        sqlx::query("UPDATE workspaces SET previous = ? WHERE task_key = ?")
            .bind(serde_json::to_string(previous)?)
            .bind(key.as_str())
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// For a task working in the project's checkout: where each folder was before.
    pub async fn checkout_previous(
        &self,
        key: &TaskKey,
    ) -> Result<Option<Vec<(String, String)>>, StoreError> {
        let json: Option<String> =
            sqlx::query_scalar("SELECT previous FROM workspaces WHERE task_key = ?")
                .bind(key.as_str())
                .fetch_optional(&self.pool)
                .await?
                .flatten();
        Ok(json.map(|j| serde_json::from_str(&j)).transpose()?)
    }

    /// The task registered in this checkout, if any, and whether it has finished
    /// (done or failed). Only one unfinished task works in a checkout at a time.
    pub async fn checkout_user(
        &self,
        path: &std::path::Path,
    ) -> Result<Option<(String, bool)>, StoreError> {
        let row: Option<(String, String)> = sqlx::query_as(
            "SELECT w.task_key, t.status FROM workspaces w JOIN tasks t ON t.key = w.task_key
             WHERE w.previous IS NOT NULL AND w.path = ?",
        )
        .bind(path.to_string_lossy())
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|(key, status)| {
            let finished = serde_json::from_str::<delilah_core::TaskStatus>(&status)
                .is_ok_and(|s| s.is_terminal());
            (key, finished)
        }))
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

    #[tokio::test]
    async fn a_finished_task_no_longer_holds_the_checkout() {
        use chrono::Utc;
        use delilah_core::Event;
        let store = Store::in_memory().await.unwrap();
        let key = TaskKey::new("SEC-1");
        store
            .create_task(&Task::new(key.clone(), "t", "feature"))
            .await
            .unwrap();
        let ws = Workspace {
            path: "/w/delilah".into(),
            branch: "feature/SEC-1".into(),
            kind: ProjectKind::Repo,
            submodules: vec![],
        };
        store
            .save_workspace(&key, "/w/delilah".as_ref(), &ws)
            .await
            .unwrap();
        store
            .set_checkout_previous(&key, &[(".".into(), "main".into())])
            .await
            .unwrap();
        for event in [Event::Start, Event::WorkspaceReady] {
            store.apply_event(&key, event, Utc::now()).await.unwrap();
        }
        let path = std::path::Path::new("/w/delilah");
        assert_eq!(
            store.checkout_user(path).await.unwrap(),
            Some(("SEC-1".into(), false))
        );

        store
            .apply_event(&key, Event::MarkDone, Utc::now())
            .await
            .unwrap();
        assert_eq!(
            store.checkout_user(path).await.unwrap(),
            Some(("SEC-1".into(), true))
        );
        assert_eq!(
            store.checkout_user("/elsewhere".as_ref()).await.unwrap(),
            None
        );
    }
}
