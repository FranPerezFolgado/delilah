//! The avisos the user removed from the inbox. `inbox_dismissals` is the only record of a
//! dismissal, also for the items backed by a PR notice.

use std::collections::HashSet;

use chrono::{DateTime, Utc};

use crate::store::{Store, StoreError};

impl Store {
    /// Records `(item id, task key)` pairs as dismissed. A task that does not exist (a
    /// project's session) is ignored, and so is an id dismissed already.
    pub(crate) async fn dismiss(&self, items: &[(String, String)]) -> Result<(), StoreError> {
        let mut tx = self.pool.begin().await?;
        for (id, task) in items {
            sqlx::query(
                "INSERT OR IGNORE INTO inbox_dismissals (id, task_key, dismissed_at)
                 SELECT ?, key, ? FROM tasks WHERE key = ?",
            )
            .bind(id)
            .bind(Utc::now().to_rfc3339())
            .bind(task)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    /// Forgets the dismissals of `ids`; unknown ids are ignored.
    pub(crate) async fn undismiss(&self, ids: &[String]) -> Result<(), StoreError> {
        let mut tx = self.pool.begin().await?;
        for id in ids {
            sqlx::query("DELETE FROM inbox_dismissals WHERE id = ?")
                .bind(id)
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    pub(crate) async fn dismissed(&self) -> Result<HashSet<String>, StoreError> {
        let ids: Vec<String> = sqlx::query_scalar("SELECT id FROM inbox_dismissals")
            .fetch_all(&self.pool)
            .await?;
        Ok(ids.into_iter().collect())
    }

    /// Drops the dismissals made before `older_than`; how many.
    pub(crate) async fn purge_dismissals(
        &self,
        older_than: DateTime<Utc>,
    ) -> Result<u64, StoreError> {
        let result = sqlx::query("DELETE FROM inbox_dismissals WHERE dismissed_at < ?")
            .bind(older_than.to_rfc3339())
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected())
    }
}

#[cfg(test)]
mod tests {
    use chrono::Duration;
    use delilah_core::{Task, TaskKey};

    use super::*;

    async fn store() -> Store {
        let store = Store::in_memory().await.unwrap();
        store
            .create_task(&Task::new(TaskKey::new("PROJ-1"), "Fix it", "feature"))
            .await
            .unwrap();
        store
    }

    fn item(id: &str) -> (String, String) {
        (id.into(), "PROJ-1".into())
    }

    #[tokio::test]
    async fn dismiss_and_undismiss_round_trip() {
        let store = store().await;
        store.dismiss(&[item("a"), item("b")]).await.unwrap();
        store.dismiss(&[item("a")]).await.unwrap();
        assert_eq!(store.dismissed().await.unwrap().len(), 2);
        store.undismiss(&["a".into(), "nope".into()]).await.unwrap();
        let left = store.dismissed().await.unwrap();
        assert_eq!(left, HashSet::from(["b".to_string()]));
    }

    #[tokio::test]
    async fn an_unknown_task_is_ignored() {
        let store = store().await;
        store
            .dismiss(&[
                ("x".into(), "@project".into()),
                ("y".into(), "NOPE-1".into()),
            ])
            .await
            .unwrap();
        assert!(store.dismissed().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn rows_go_with_the_task() {
        let store = store().await;
        store.dismiss(&[item("a")]).await.unwrap();
        store.delete_task(&TaskKey::new("PROJ-1")).await.unwrap();
        assert!(store.dismissed().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn purge_removes_only_rows_older_than_the_cutoff() {
        let store = store().await;
        store.dismiss(&[item("old"), item("new")]).await.unwrap();
        let old = (Utc::now() - Duration::days(31)).to_rfc3339();
        sqlx::query("UPDATE inbox_dismissals SET dismissed_at = ? WHERE id = 'old'")
            .bind(old)
            .execute(&store.pool)
            .await
            .unwrap();
        let purged = store
            .purge_dismissals(Utc::now() - Duration::days(30))
            .await
            .unwrap();
        assert_eq!(purged, 1);
        assert_eq!(
            store.dismissed().await.unwrap(),
            HashSet::from(["new".to_string()])
        );
    }
}
