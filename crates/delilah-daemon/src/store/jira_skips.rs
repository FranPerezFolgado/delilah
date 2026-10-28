//! Jira sub-tasks the user removed from a parent task, so the import does not recreate them.

use std::collections::HashSet;

use chrono::Utc;
use delilah_core::TaskKey;
use sqlx::SqliteConnection;

use crate::store::{Store, StoreError};

/// Records Jira keys to skip under `parent`; keys already recorded are left alone.
/// Takes a connection so the import can write it inside its transaction.
pub(crate) async fn add_jira_skips(
    conn: &mut SqliteConnection,
    parent: &TaskKey,
    keys: &HashSet<String>,
) -> Result<(), StoreError> {
    let now = Utc::now().to_rfc3339();
    for key in keys {
        sqlx::query(
            "INSERT OR IGNORE INTO jira_subtask_skips (parent_task, jira_key, created_at) \
             VALUES (?, ?, ?)",
        )
        .bind(parent.as_str())
        .bind(key)
        .bind(&now)
        .execute(&mut *conn)
        .await?;
    }
    Ok(())
}

/// Forgets skipped Jira keys under `parent`, so the next import brings them back.
pub(crate) async fn remove_jira_skips(
    conn: &mut SqliteConnection,
    parent: &TaskKey,
    keys: &HashSet<String>,
) -> Result<(), StoreError> {
    for key in keys {
        sqlx::query("DELETE FROM jira_subtask_skips WHERE parent_task = ? AND jira_key = ?")
            .bind(parent.as_str())
            .bind(key)
            .execute(&mut *conn)
            .await?;
    }
    Ok(())
}

impl Store {
    /// The Jira keys skipped under `parent`, sorted.
    pub(crate) async fn jira_skips_of(&self, parent: &TaskKey) -> Result<Vec<String>, StoreError> {
        let rows: Vec<(String,)> = sqlx::query_as(
            "SELECT jira_key FROM jira_subtask_skips WHERE parent_task = ? ORDER BY jira_key",
        )
        .bind(parent.as_str())
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(|(k,)| k).collect())
    }

    /// Drops every skip of a deleted parent task (archiving keeps them).
    pub(crate) async fn delete_jira_skips(&self, parent: &TaskKey) -> Result<(), StoreError> {
        sqlx::query("DELETE FROM jira_subtask_skips WHERE parent_task = ?")
            .bind(parent.as_str())
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set(keys: &[&str]) -> HashSet<String> {
        keys.iter().map(|k| k.to_string()).collect()
    }

    #[tokio::test]
    async fn add_idempotent() {
        let store = Store::in_memory().await.unwrap();
        let p = TaskKey::new("P-1");
        add_jira_skips(
            &mut store.pool.acquire().await.unwrap(),
            &p,
            &set(&["J-1", "J-2"]),
        )
        .await
        .unwrap();
        add_jira_skips(
            &mut store.pool.acquire().await.unwrap(),
            &p,
            &set(&["J-2", "J-3"]),
        )
        .await
        .unwrap();
        assert_eq!(
            store.jira_skips_of(&p).await.unwrap(),
            ["J-1", "J-2", "J-3"]
        );
    }

    #[tokio::test]
    async fn remove() {
        let store = Store::in_memory().await.unwrap();
        let p = TaskKey::new("P-1");
        add_jira_skips(
            &mut store.pool.acquire().await.unwrap(),
            &p,
            &set(&["J-1", "J-2"]),
        )
        .await
        .unwrap();
        remove_jira_skips(
            &mut store.pool.acquire().await.unwrap(),
            &p,
            &set(&["J-1", "J-9"]),
        )
        .await
        .unwrap();
        assert_eq!(store.jira_skips_of(&p).await.unwrap(), ["J-2"]);
    }

    #[tokio::test]
    async fn jira_skips_sorted() {
        let store = Store::in_memory().await.unwrap();
        let p = TaskKey::new("P-1");
        add_jira_skips(
            &mut store.pool.acquire().await.unwrap(),
            &p,
            &set(&["J-3", "J-1", "J-2"]),
        )
        .await
        .unwrap();
        assert_eq!(
            store.jira_skips_of(&p).await.unwrap(),
            ["J-1", "J-2", "J-3"]
        );
    }

    #[tokio::test]
    async fn delete_single_parent_only() {
        let store = Store::in_memory().await.unwrap();
        let (a, b) = (TaskKey::new("P-1"), TaskKey::new("P-2"));
        add_jira_skips(&mut store.pool.acquire().await.unwrap(), &a, &set(&["J-1"]))
            .await
            .unwrap();
        add_jira_skips(&mut store.pool.acquire().await.unwrap(), &b, &set(&["J-1"]))
            .await
            .unwrap();
        store.delete_jira_skips(&a).await.unwrap();
        assert!(store.jira_skips_of(&a).await.unwrap().is_empty());
        assert_eq!(store.jira_skips_of(&b).await.unwrap(), ["J-1"]);
    }
}
