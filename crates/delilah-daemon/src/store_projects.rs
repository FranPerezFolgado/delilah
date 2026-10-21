//! Registered projects: the codebases tasks run against.

use chrono::Utc;
use delilah_core::{Project, ProjectKind, ProjectSettings};
use sqlx::Row;
use sqlx::sqlite::SqliteRow;

use crate::store::{Store, StoreError};

impl Store {
    pub async fn create_project(&self, project: &Project) -> Result<(), StoreError> {
        let result = sqlx::query(
            "INSERT INTO projects (name, path, kind, workflow, settings, created_at) VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(&project.name)
        .bind(project.path.to_string_lossy())
        .bind(project.kind.name())
        .bind(&project.workflow)
        .bind(serde_json::to_string(&project.settings)?)
        .bind(Utc::now().to_rfc3339())
        .execute(&self.pool)
        .await;
        match result {
            Err(sqlx::Error::Database(e)) if e.is_unique_violation() => {
                Err(StoreError::ProjectExists(project.name.clone()))
            }
            other => other.map(|_| ()).map_err(Into::into),
        }
    }

    pub async fn list_projects(&self) -> Result<Vec<Project>, StoreError> {
        let rows = sqlx::query("SELECT name, path, kind, workflow, settings FROM projects ORDER BY name")
            .fetch_all(&self.pool)
            .await?;
        rows.iter().map(project_from_row).collect()
    }

    pub async fn project(&self, name: &str) -> Result<Project, StoreError> {
        let row = sqlx::query("SELECT name, path, kind, workflow, settings FROM projects WHERE name = ?")
            .bind(name)
            .fetch_optional(&self.pool)
            .await?
            .ok_or_else(|| StoreError::ProjectNotFound(name.to_string()))?;
        project_from_row(&row)
    }

    /// Replaces a project's settings.
    pub async fn set_project_settings(
        &self,
        name: &str,
        settings: &ProjectSettings,
    ) -> Result<(), StoreError> {
        let updated = sqlx::query("UPDATE projects SET settings = ? WHERE name = ?")
            .bind(serde_json::to_string(settings)?)
            .bind(name)
            .execute(&self.pool)
            .await?;
        if updated.rows_affected() == 0 {
            return Err(StoreError::ProjectNotFound(name.to_string()));
        }
        Ok(())
    }
}

impl Store {
    /// Changes a project, renaming it everywhere it is referenced if its name changes.
    /// Its settings are left alone: they change only through `set_project_settings`.
    pub async fn update_project(
        &self,
        old_name: &str,
        project: &Project,
    ) -> Result<(), StoreError> {
        let mut tx = self.pool.begin().await?;
        // Tasks and sessions point at the name: check the references at commit.
        sqlx::query("PRAGMA defer_foreign_keys = ON")
            .execute(&mut *tx)
            .await?;
        let updated = sqlx::query(
            "UPDATE projects SET name = ?, path = ?, kind = ?, workflow = ? WHERE name = ?",
        )
        .bind(&project.name)
        .bind(project.path.to_string_lossy())
        .bind(project.kind.name())
        .bind(&project.workflow)
        .bind(old_name)
        .execute(&mut *tx)
        .await;
        match updated {
            Err(sqlx::Error::Database(e)) if e.is_unique_violation() => {
                return Err(StoreError::ProjectExists(project.name.clone()));
            }
            other => {
                if other?.rows_affected() == 0 {
                    return Err(StoreError::ProjectNotFound(old_name.to_string()));
                }
            }
        }
        if project.name != old_name {
            for table in [
                "UPDATE tasks SET project = ? WHERE project = ?",
                "UPDATE sessions SET project = ? WHERE project = ?",
            ] {
                sqlx::query(table)
                    .bind(&project.name)
                    .bind(old_name)
                    .execute(&mut *tx)
                    .await?;
            }
        }
        tx.commit().await?;
        Ok(())
    }

    /// Why a project cannot be deleted now: a task of it still has a workspace.
    pub async fn project_in_use(&self, name: &str) -> Result<Option<String>, StoreError> {
        Ok(sqlx::query_scalar(
            "SELECT t.key FROM tasks t JOIN workspaces w ON w.task_key = t.key WHERE t.project = ? LIMIT 1",
        )
        .bind(name)
        .fetch_optional(&self.pool)
        .await?)
    }

    /// Deletes a project and its own sessions; its (finished) tasks stay, without it.
    pub async fn delete_project(&self, name: &str) -> Result<(), StoreError> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("UPDATE tasks SET project = NULL WHERE project = ?")
            .bind(name)
            .execute(&mut *tx)
            .await?;
        sqlx::query("DELETE FROM sessions WHERE project = ?")
            .bind(name)
            .execute(&mut *tx)
            .await?;
        let deleted = sqlx::query("DELETE FROM projects WHERE name = ?")
            .bind(name)
            .execute(&mut *tx)
            .await?;
        if deleted.rows_affected() == 0 {
            return Err(StoreError::ProjectNotFound(name.to_string()));
        }
        tx.commit().await?;
        Ok(())
    }
}

fn project_from_row(row: &SqliteRow) -> Result<Project, StoreError> {
    let kind: String = row.try_get("kind")?;
    let name: String = row.try_get("name")?;
    let stored: String = row.try_get("settings")?;
    let settings = serde_json::from_str(&stored).unwrap_or_else(|e| {
        tracing::warn!(project = %name, "unreadable project settings: {e}; using defaults");
        ProjectSettings::default()
    });
    Ok(Project {
        name,
        path: row.try_get::<String, _>("path")?.into(),
        kind: ProjectKind::parse(&kind)
            .ok_or_else(|| StoreError::Corrupt(format!("unknown project kind `{kind}`")))?,
        workflow: row.try_get("workflow")?,
        settings,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project(name: &str, path: &str, kind: ProjectKind) -> Project {
        Project {
            name: name.into(),
            path: path.into(),
            kind,
            workflow: "feature".into(),
            settings: Default::default(),
        }
    }

    #[tokio::test]
    async fn keeps_projects_and_rejects_duplicate_names_or_folders() {
        let store = Store::in_memory().await.unwrap();
        let work = project("plataforma", "/w/plataforma", ProjectKind::Parent);
        store.create_project(&work).await.unwrap();
        store
            .create_project(&project("signal", "/p/signal", ProjectKind::Repo))
            .await
            .unwrap();

        let same_name = project("plataforma", "/elsewhere", ProjectKind::Repo);
        assert!(matches!(
            store.create_project(&same_name).await,
            Err(StoreError::ProjectExists(_))
        ));
        let same_folder = project("other", "/w/plataforma", ProjectKind::Repo);
        assert!(matches!(
            store.create_project(&same_folder).await,
            Err(StoreError::ProjectExists(_))
        ));

        assert_eq!(store.project("plataforma").await.unwrap(), work);
        assert_eq!(store.list_projects().await.unwrap().len(), 2);
        assert!(matches!(
            store.project("nope").await,
            Err(StoreError::ProjectNotFound(_))
        ));
    }

    #[tokio::test]
    async fn renames_carry_tasks_along_and_deletes_keep_finished_tasks() {
        use delilah_core::{Task, TaskKey};
        let store = Store::in_memory().await.unwrap();
        store
            .create_project(&project("plataforma", "/w/p", ProjectKind::Repo))
            .await
            .unwrap();
        let mut task = Task::new(TaskKey::new("PROJ-1"), "t", "feature");
        task.project = Some("plataforma".into());
        store.create_task(&task).await.unwrap();

        let renamed = project("payments", "/w/p", ProjectKind::Repo);
        store.update_project("plataforma", &renamed).await.unwrap();
        let moved = store
            .get_task(&TaskKey::new("PROJ-1"))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(moved.project.as_deref(), Some("payments"));

        assert_eq!(store.project_in_use("payments").await.unwrap(), None);
        store.delete_project("payments").await.unwrap();
        let kept = store
            .get_task(&TaskKey::new("PROJ-1"))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(kept.project, None);
        assert!(store.list_projects().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn settings_default_round_trip_survive_rename_and_tolerate_garbage() {
        let store = Store::in_memory().await.unwrap();
        store
            .create_project(&project("a", "/w/a", ProjectKind::Repo))
            .await
            .unwrap();
        assert_eq!(
            store.project("a").await.unwrap().settings,
            ProjectSettings::default()
        );

        let settings: ProjectSettings =
            serde_json::from_str(r#"{"setup":"make setup"}"#).unwrap();
        assert_ne!(settings, ProjectSettings::default());
        store.set_project_settings("a", &settings).await.unwrap();
        assert_eq!(store.project("a").await.unwrap().settings, settings);
        assert!(matches!(
            store.set_project_settings("nope", &settings).await,
            Err(StoreError::ProjectNotFound(_))
        ));

        store
            .update_project("a", &project("b", "/w/a", ProjectKind::Repo))
            .await
            .unwrap();
        assert_eq!(store.project("b").await.unwrap().settings, settings);

        sqlx::query("UPDATE projects SET settings = 'not json' WHERE name = 'b'")
            .execute(&store.pool)
            .await
            .unwrap();
        assert_eq!(
            store.project("b").await.unwrap().settings,
            ProjectSettings::default()
        );
        assert_eq!(store.list_projects().await.unwrap().len(), 1);
    }
}
