//! Registered projects: the codebases tasks run against.

use chrono::Utc;
use delilah_core::{Project, ProjectKind, ProjectSettings};
use sqlx::Row;
use sqlx::sqlite::SqliteRow;

use crate::store::{Store, StoreError};

impl Store {
    pub async fn create_project(&self, project: &Project) -> Result<(), StoreError> {
        let result = sqlx::query(
            "INSERT INTO projects (name, path, kind, workflow, settings, preview_command, created_at) VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&project.name)
        .bind(project.path.to_string_lossy())
        .bind(project.kind.name())
        .bind(&project.workflow)
        .bind(serde_json::to_string(&project.settings)?)
        .bind(blank_to_none(project.preview_command.as_deref()))
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
        let rows = sqlx::query(
            "SELECT name, path, kind, workflow, settings, preview_command FROM projects ORDER BY name",
        )
        .fetch_all(&self.pool)
        .await?;
        rows.iter().map(project_from_row).collect()
    }

    pub async fn project(&self, name: &str) -> Result<Project, StoreError> {
        let row = sqlx::query(
            "SELECT name, path, kind, workflow, settings, preview_command FROM projects WHERE name = ?",
        )
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
    /// Sets the command that starts the project's preview; empty or blank clears it.
    pub async fn set_preview_command(
        &self,
        name: &str,
        command: Option<&str>,
    ) -> Result<(), StoreError> {
        let updated = sqlx::query("UPDATE projects SET preview_command = ? WHERE name = ?")
            .bind(blank_to_none(command))
            .bind(name)
            .execute(&self.pool)
            .await?;
        if updated.rows_affected() == 0 {
            return Err(StoreError::ProjectNotFound(name.to_string()));
        }
        Ok(())
    }

    /// Changes a project, renaming it everywhere it is referenced if its name changes.
    /// Its settings are left alone: they change only through `set_project_settings`.
    /// The preview command is not touched here: it has its own `set_preview_command`.
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

/// A blank command means none: stored as NULL.
fn blank_to_none(command: Option<&str>) -> Option<&str> {
    command.filter(|c| !c.trim().is_empty())
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
        preview_command: row.try_get("preview_command")?,
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
            preview_command: None,
        }
    }

    #[tokio::test]
    async fn keeps_projects_and_rejects_duplicate_names_or_folders() {
        let store = Store::in_memory().await.unwrap();
        let work = project("platform", "/w/platform", ProjectKind::Parent);
        store.create_project(&work).await.unwrap();
        store
            .create_project(&project("signal", "/p/signal", ProjectKind::Repo))
            .await
            .unwrap();

        let same_name = project("platform", "/elsewhere", ProjectKind::Repo);
        assert!(matches!(
            store.create_project(&same_name).await,
            Err(StoreError::ProjectExists(_))
        ));
        let same_folder = project("other", "/w/platform", ProjectKind::Repo);
        assert!(matches!(
            store.create_project(&same_folder).await,
            Err(StoreError::ProjectExists(_))
        ));

        assert_eq!(store.project("platform").await.unwrap(), work);
        assert_eq!(store.list_projects().await.unwrap().len(), 2);
        assert!(matches!(
            store.project("nope").await,
            Err(StoreError::ProjectNotFound(_))
        ));
    }

    #[tokio::test]
    async fn preview_command_is_stored_and_blank_means_none() {
        let store = Store::in_memory().await.unwrap();
        store
            .create_project(&project("p", "/w/p", ProjectKind::Repo))
            .await
            .unwrap();
        assert_eq!(store.project("p").await.unwrap().preview_command, None);
        store
            .set_preview_command("p", Some("npm run dev"))
            .await
            .unwrap();
        assert_eq!(
            store.project("p").await.unwrap().preview_command.as_deref(),
            Some("npm run dev")
        );
        assert_eq!(
            store.list_projects().await.unwrap()[0]
                .preview_command
                .as_deref(),
            Some("npm run dev")
        );
        store.set_preview_command("p", Some("  ")).await.unwrap();
        assert_eq!(store.project("p").await.unwrap().preview_command, None);
        store.set_preview_command("p", Some("x")).await.unwrap();
        store.set_preview_command("p", Some("")).await.unwrap();
        assert_eq!(store.project("p").await.unwrap().preview_command, None);
        assert!(matches!(
            store.set_preview_command("nope", None).await,
            Err(StoreError::ProjectNotFound(_))
        ));
    }

    #[tokio::test]
    async fn renames_carry_tasks_along_and_deletes_keep_finished_tasks() {
        use delilah_core::{Task, TaskKey};
        let store = Store::in_memory().await.unwrap();
        store
            .create_project(&project("platform", "/w/p", ProjectKind::Repo))
            .await
            .unwrap();
        let mut task = Task::new(TaskKey::new("PROJ-1"), "t", "feature");
        task.project = Some("platform".into());
        store.create_task(&task).await.unwrap();

        let renamed = project("payments", "/w/p", ProjectKind::Repo);
        store.update_project("platform", &renamed).await.unwrap();
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

        let settings: ProjectSettings = serde_json::from_str(r#"{"setup":"make setup"}"#).unwrap();
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

    #[tokio::test]
    async fn update_project_keeps_the_preview_command() {
        let store = Store::in_memory().await.unwrap();
        store
            .create_project(&project("web", "/w/web", ProjectKind::Repo))
            .await
            .unwrap();
        store
            .set_preview_command("web", Some("npm run dev"))
            .await
            .unwrap();
        // A caller that builds the project without the command must not wipe it.
        let renamed = project("site", "/w/web", ProjectKind::Repo);
        store.update_project("web", &renamed).await.unwrap();
        assert_eq!(
            store
                .project("site")
                .await
                .unwrap()
                .preview_command
                .as_deref(),
            Some("npm run dev")
        );
    }
}
