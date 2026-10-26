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

    /// Sets the branch of a workspace saved without one, leaving the rest as it is.
    pub async fn set_workspace_branch(
        &self,
        key: &TaskKey,
        branch: &str,
    ) -> Result<(), StoreError> {
        sqlx::query("UPDATE workspaces SET branch = ? WHERE task_key = ?")
            .bind(branch)
            .bind(key.as_str())
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Records the spec folders present when a workspace started without a branch
    /// (`label:name`); `None` for one started with a branch.
    pub async fn set_spec_dirs_at_start(
        &self,
        key: &TaskKey,
        dirs: Option<&[String]>,
    ) -> Result<(), StoreError> {
        sqlx::query("UPDATE workspaces SET spec_dirs_at_start = ? WHERE task_key = ?")
            .bind(dirs.map(serde_json::to_string).transpose()?)
            .bind(key.as_str())
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Marks a workspace that started without a branch (decision 65): the only kind that
    /// gets the branch steps (adopt, create, guard) from its first analysis.
    pub async fn set_started_branchless(
        &self,
        key: &TaskKey,
        branchless: bool,
    ) -> Result<(), StoreError> {
        sqlx::query("UPDATE workspaces SET started_branchless = ? WHERE task_key = ?")
            .bind(branchless)
            .bind(key.as_str())
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Whether the workspace started without a branch (see [`Store::set_started_branchless`]).
    pub async fn started_branchless(&self, key: &TaskKey) -> Result<bool, StoreError> {
        let flag: Option<bool> =
            sqlx::query_scalar("SELECT started_branchless FROM workspaces WHERE task_key = ?")
                .bind(key.as_str())
                .fetch_optional(&self.pool)
                .await?;
        Ok(flag.unwrap_or(false))
    }

    /// The spec folders recorded at start (see [`Store::set_spec_dirs_at_start`]).
    pub async fn spec_dirs_at_start(
        &self,
        key: &TaskKey,
    ) -> Result<Option<Vec<String>>, StoreError> {
        let json: Option<String> =
            sqlx::query_scalar("SELECT spec_dirs_at_start FROM workspaces WHERE task_key = ?")
                .bind(key.as_str())
                .fetch_optional(&self.pool)
                .await?
                .flatten();
        Ok(json.map(|j| serde_json::from_str(&j)).transpose()?)
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
            .create_task(&Task::new(key.clone(), "t", "project"))
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
    async fn the_started_branchless_mark_defaults_off_and_survives_the_branch() {
        let store = Store::in_memory().await.unwrap();
        let key = TaskKey::new("LIBRE-1");
        store
            .create_task(&Task::new(key.clone(), "t", "project"))
            .await
            .unwrap();
        let ws = Workspace {
            path: "/tmp/ws/LIBRE-1".into(),
            branch: String::new(),
            kind: ProjectKind::Repo,
            submodules: vec![],
        };
        store
            .save_workspace(&key, "/tmp/parent".as_ref(), &ws)
            .await
            .unwrap();
        assert!(!store.started_branchless(&key).await.unwrap());
        store.set_started_branchless(&key, true).await.unwrap();
        store.set_workspace_branch(&key, "003-x").await.unwrap();
        assert!(store.started_branchless(&key).await.unwrap());
    }

    #[tokio::test]
    async fn a_workspace_without_branch_keeps_its_spec_dirs_and_gets_the_branch_later() {
        let store = Store::in_memory().await.unwrap();
        let key = TaskKey::new("PROJ-1");
        store
            .create_task(&Task::new(key.clone(), "t", "project"))
            .await
            .unwrap();
        let ws = Workspace {
            path: "/tmp/ws/PROJ-1".into(),
            branch: String::new(),
            kind: ProjectKind::Repo,
            submodules: vec![],
        };
        store
            .save_workspace(&key, "/tmp/parent".as_ref(), &ws)
            .await
            .unwrap();
        // Saved without spec dirs: reads None.
        assert_eq!(store.spec_dirs_at_start(&key).await.unwrap(), None);

        let dirs = vec![".:001-a".to_string(), "svc:002-b".to_string()];
        store
            .set_spec_dirs_at_start(&key, Some(&dirs))
            .await
            .unwrap();
        assert_eq!(
            store.spec_dirs_at_start(&key).await.unwrap(),
            Some(dirs.clone())
        );

        store.set_workspace_branch(&key, "003-x").await.unwrap();
        let (parent, got) = store.workspace(&key).await.unwrap().unwrap();
        assert_eq!(parent, std::path::PathBuf::from("/tmp/parent"));
        assert_eq!(got.branch, "003-x");
        assert_eq!(got.path, ws.path);
        assert_eq!(store.spec_dirs_at_start(&key).await.unwrap(), Some(dirs));
    }

    #[tokio::test]
    async fn a_finished_task_no_longer_holds_the_checkout() {
        use chrono::Utc;
        use delilah_core::Event;
        let store = Store::in_memory().await.unwrap();
        let key = TaskKey::new("SEC-1");
        store
            .create_task(&Task::new(key.clone(), "t", "project"))
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
