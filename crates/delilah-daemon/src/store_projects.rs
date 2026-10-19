//! Registered projects: the codebases tasks run against.

use chrono::Utc;
use delilah_core::{Project, ProjectKind};
use sqlx::Row;
use sqlx::sqlite::SqliteRow;

use crate::store::{Store, StoreError};

impl Store {
    pub async fn create_project(&self, project: &Project) -> Result<(), StoreError> {
        let result = sqlx::query(
            "INSERT INTO projects (name, path, kind, workflow, created_at) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(&project.name)
        .bind(project.path.to_string_lossy())
        .bind(project.kind.name())
        .bind(&project.workflow)
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
        let rows = sqlx::query("SELECT name, path, kind, workflow FROM projects ORDER BY name")
            .fetch_all(&self.pool)
            .await?;
        rows.iter().map(project_from_row).collect()
    }

    pub async fn project(&self, name: &str) -> Result<Project, StoreError> {
        let row = sqlx::query("SELECT name, path, kind, workflow FROM projects WHERE name = ?")
            .bind(name)
            .fetch_optional(&self.pool)
            .await?
            .ok_or_else(|| StoreError::ProjectNotFound(name.to_string()))?;
        project_from_row(&row)
    }
}

fn project_from_row(row: &SqliteRow) -> Result<Project, StoreError> {
    let kind: String = row.try_get("kind")?;
    Ok(Project {
        name: row.try_get("name")?,
        path: row.try_get::<String, _>("path")?.into(),
        kind: ProjectKind::parse(&kind)
            .ok_or_else(|| StoreError::Corrupt(format!("unknown project kind `{kind}`")))?,
        workflow: row.try_get("workflow")?,
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
}
