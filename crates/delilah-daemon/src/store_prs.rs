//! Pull requests linked to tasks, what was last seen of them, and the notices they
//! raised (see `prs.rs`).

use chrono::{DateTime, Utc};
use delilah_core::{TaskKey, TaskStatus};
use serde_json::Value;
use sqlx::Row;
use sqlx::sqlite::SqliteRow;

use crate::pr_types::{Marks, PrState, Snapshot};
use crate::store::{Store, StoreError};

/// One PR linked to a task, per repo of its workspace.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PrRow {
    pub id: i64,
    pub task_key: String,
    pub repo: String,
    pub host: String,
    pub owner_name: String,
    pub number: i64,
    pub url: String,
    /// `detected`, `opened` or `pasted`.
    pub linked_by: String,
    pub snapshot: Option<Snapshot>,
    pub marks: Marks,
    pub checked_at: Option<DateTime<Utc>>,
    pub error: Option<String>,
    /// The kind of problem behind `error` (see `AccessProblem::kind`).
    pub error_kind: Option<String>,
    pub created_at: DateTime<Utc>,
}

/// What the inbox shows for a PR.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct NoticeRow {
    pub id: i64,
    pub task_key: String,
    pub repo: String,
    pub kind: String,
    pub cause: String,
    pub text: String,
    pub detail: Option<Value>,
    pub created_at: DateTime<Utc>,
    pub resolved_at: Option<DateTime<Utc>>,
}

/// A PR to link.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct NewPr {
    pub repo: String,
    pub host: String,
    pub owner_name: String,
    pub number: i64,
    pub url: String,
    pub linked_by: String,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct NewNotice {
    pub repo: String,
    pub kind: String,
    pub cause: String,
    pub text: String,
    pub detail: Option<Value>,
}

const PR_COLUMNS: &str = "id, task_key, repo, host, owner_name, number, url, linked_by, snapshot, \
     ci_failed_sha, review_seen, issue_comment_seen, review_comment_seen, checked_at, error, error_kind, created_at";
const NOTICE_COLUMNS: &str =
    "id, task_key, repo, kind, cause, text, detail, created_at, resolved_at";

fn time(raw: &str) -> Result<DateTime<Utc>, StoreError> {
    DateTime::parse_from_rfc3339(raw)
        .map(|t| t.with_timezone(&Utc))
        .map_err(|e| StoreError::Corrupt(format!("bad time `{raw}`: {e}")))
}

fn opt_time(raw: Option<String>) -> Result<Option<DateTime<Utc>>, StoreError> {
    raw.as_deref().map(time).transpose()
}

fn pr_row(row: &SqliteRow) -> Result<PrRow, StoreError> {
    let snapshot: Option<String> = row.try_get("snapshot")?;
    Ok(PrRow {
        id: row.try_get("id")?,
        task_key: row.try_get("task_key")?,
        repo: row.try_get("repo")?,
        host: row.try_get("host")?,
        owner_name: row.try_get("owner_name")?,
        number: row.try_get("number")?,
        url: row.try_get("url")?,
        linked_by: row.try_get("linked_by")?,
        snapshot: snapshot.map(|s| serde_json::from_str(&s)).transpose()?,
        marks: Marks {
            ci_failed_sha: row.try_get("ci_failed_sha")?,
            review: row.try_get("review_seen")?,
            issue_comment: row.try_get("issue_comment_seen")?,
            review_comment: row.try_get("review_comment_seen")?,
        },
        checked_at: opt_time(row.try_get("checked_at")?)?,
        error: row.try_get("error")?,
        error_kind: row.try_get("error_kind")?,
        created_at: time(&row.try_get::<String, _>("created_at")?)?,
    })
}

fn notice_row(row: &SqliteRow) -> Result<NoticeRow, StoreError> {
    let detail: Option<String> = row.try_get("detail")?;
    Ok(NoticeRow {
        id: row.try_get("id")?,
        task_key: row.try_get("task_key")?,
        repo: row.try_get("repo")?,
        kind: row.try_get("kind")?,
        cause: row.try_get("cause")?,
        text: row.try_get("text")?,
        detail: detail.map(|d| serde_json::from_str(&d)).transpose()?,
        created_at: time(&row.try_get::<String, _>("created_at")?)?,
        resolved_at: opt_time(row.try_get("resolved_at")?)?,
    })
}

/// Resolves the open notices of `kinds` for a task's repo, inside a transaction.
async fn resolve_kinds(
    tx: &mut sqlx::SqliteConnection,
    key: &str,
    repo: &str,
    kinds: &[&str],
) -> Result<u64, StoreError> {
    let mut done = 0;
    for kind in kinds {
        done += sqlx::query(
            "UPDATE pr_notices SET resolved_at = ?
             WHERE task_key = ? AND repo = ? AND kind = ? AND resolved_at IS NULL",
        )
        .bind(Utc::now().to_rfc3339())
        .bind(key)
        .bind(repo)
        .bind(kind)
        .execute(&mut *tx)
        .await?
        .rows_affected();
    }
    Ok(done)
}

async fn insert_notice_in(
    tx: &mut sqlx::SqliteConnection,
    key: &str,
    n: &NewNotice,
) -> Result<bool, StoreError> {
    let detail = n.detail.as_ref().map(serde_json::to_string).transpose()?;
    let result = sqlx::query(
        "INSERT OR IGNORE INTO pr_notices (task_key, repo, kind, cause, text, detail, created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(key)
    .bind(&n.repo)
    .bind(&n.kind)
    .bind(&n.cause)
    .bind(&n.text)
    .bind(detail)
    .bind(Utc::now().to_rfc3339())
    .execute(&mut *tx)
    .await?;
    Ok(result.rows_affected() > 0)
}

impl Store {
    /// Links a PR to a task's repo, replacing the one there. Linking the same URL again
    /// keeps what was seen of it; a different one starts from nothing.
    pub(crate) async fn link_pr(&self, key: &TaskKey, pr: &NewPr) -> Result<PrRow, StoreError> {
        let mut tx = self.pool.begin().await?;
        // A different PR leaves the notices of the old one meaningless.
        let old_url: Option<String> =
            sqlx::query_scalar("SELECT url FROM pull_requests WHERE task_key = ? AND repo = ?")
                .bind(key.as_str())
                .bind(&pr.repo)
                .fetch_optional(&mut *tx)
                .await?;
        if old_url.is_some_and(|old| old != pr.url) {
            sqlx::query(
                "UPDATE pr_notices SET resolved_at = ?
                 WHERE task_key = ? AND repo = ? AND resolved_at IS NULL",
            )
            .bind(Utc::now().to_rfc3339())
            .bind(key.as_str())
            .bind(&pr.repo)
            .execute(&mut *tx)
            .await?;
        }
        // Never INSERT OR REPLACE: it would change the id.
        sqlx::query(
            "INSERT INTO pull_requests (task_key, repo, host, owner_name, number, url, linked_by, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT (task_key, repo) DO UPDATE SET
               host = excluded.host,
               owner_name = excluded.owner_name,
               number = excluded.number,
               linked_by = excluded.linked_by,
               snapshot = CASE WHEN url = excluded.url THEN snapshot END,
               ci_failed_sha = CASE WHEN url = excluded.url THEN ci_failed_sha END,
               review_seen = CASE WHEN url = excluded.url THEN review_seen ELSE 0 END,
               issue_comment_seen = CASE WHEN url = excluded.url THEN issue_comment_seen ELSE 0 END,
               review_comment_seen = CASE WHEN url = excluded.url THEN review_comment_seen ELSE 0 END,
               checked_at = CASE WHEN url = excluded.url THEN checked_at END,
               error = CASE WHEN url = excluded.url THEN error END,
               error_kind = CASE WHEN url = excluded.url THEN error_kind END,
               url = excluded.url",
        )
        .bind(key.as_str())
        .bind(&pr.repo)
        .bind(&pr.host)
        .bind(&pr.owner_name)
        .bind(pr.number)
        .bind(&pr.url)
        .bind(&pr.linked_by)
        .bind(Utc::now().to_rfc3339())
        .execute(&mut *tx)
        .await?;
        sqlx::query("DELETE FROM pr_unlinked WHERE task_key = ? AND url = ?")
            .bind(key.as_str())
            .bind(&pr.url)
            .execute(&mut *tx)
            .await?;
        let row = sqlx::query(&format!(
            "SELECT {PR_COLUMNS} FROM pull_requests WHERE task_key = ? AND repo = ?"
        ))
        .bind(key.as_str())
        .bind(&pr.repo)
        .fetch_one(&mut *tx)
        .await?;
        tx.commit().await?;
        pr_row(&row)
    }

    pub(crate) async fn prs_of(&self, key: &TaskKey) -> Result<Vec<PrRow>, StoreError> {
        let rows = sqlx::query(&format!(
            "SELECT {PR_COLUMNS} FROM pull_requests WHERE task_key = ? ORDER BY id"
        ))
        .bind(key.as_str())
        .fetch_all(&self.pool)
        .await?;
        rows.iter().map(pr_row).collect()
    }

    pub(crate) async fn pr(&self, id: i64) -> Result<Option<PrRow>, StoreError> {
        let row = sqlx::query(&format!(
            "SELECT {PR_COLUMNS} FROM pull_requests WHERE id = ?"
        ))
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        row.as_ref().map(pr_row).transpose()
    }

    /// Deletes the link, resolves its notices and remembers the URL so detection never
    /// brings it back. Returns the row, or `None` if there was none.
    pub(crate) async fn unlink_pr(&self, id: i64) -> Result<Option<PrRow>, StoreError> {
        let Some(row) = self.pr(id).await? else {
            return Ok(None);
        };
        let mut tx = self.pool.begin().await?;
        sqlx::query("DELETE FROM pull_requests WHERE id = ?")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        sqlx::query(
            "UPDATE pr_notices SET resolved_at = ?
             WHERE task_key = ? AND repo = ? AND resolved_at IS NULL",
        )
        .bind(Utc::now().to_rfc3339())
        .bind(&row.task_key)
        .bind(&row.repo)
        .execute(&mut *tx)
        .await?;
        sqlx::query("INSERT OR IGNORE INTO pr_unlinked (task_key, url) VALUES (?, ?)")
            .bind(&row.task_key)
            .bind(&row.url)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(Some(row))
    }

    pub(crate) async fn is_unlinked(&self, key: &TaskKey, url: &str) -> Result<bool, StoreError> {
        let row = sqlx::query("SELECT 1 FROM pr_unlinked WHERE task_key = ? AND url = ?")
            .bind(key.as_str())
            .bind(url)
            .fetch_optional(&self.pool)
            .await?;
        Ok(row.is_some())
    }

    /// The PRs worth checking: not seen yet or open, of tasks not archived (a task is
    /// archived when it has no workspace and is done or failed).
    pub(crate) async fn open_prs(&self) -> Result<Vec<PrRow>, StoreError> {
        let rows = sqlx::query(&format!(
            "SELECT {}, t.status AS task_status,
                    EXISTS (SELECT 1 FROM workspaces w WHERE w.task_key = p.task_key) AS has_workspace
             FROM pull_requests p JOIN tasks t ON t.key = p.task_key
             ORDER BY p.id",
            PR_COLUMNS
                .split(", ")
                .map(|c| format!("p.{c}"))
                .collect::<Vec<_>>()
                .join(", ")
        ))
        .fetch_all(&self.pool)
        .await?;
        let mut out = Vec::new();
        for row in &rows {
            let status: TaskStatus =
                serde_json::from_str(&row.try_get::<String, _>("task_status")?)?;
            let has_workspace: bool = row.try_get("has_workspace")?;
            if status.is_terminal() && !has_workspace {
                continue;
            }
            let pr = pr_row(row)?;
            if pr
                .snapshot
                .as_ref()
                .is_some_and(|s| s.state != PrState::Open)
            {
                continue;
            }
            out.push(pr);
        }
        Ok(out)
    }

    /// Writes what a check found in one go: the snapshot, the new marks, the time, the
    /// notices it raised, and the kinds it resolved (resolved first, so a notice raised
    /// by the same check stays).
    pub(crate) async fn save_check(
        &self,
        id: i64,
        snapshot: &Snapshot,
        marks: &Marks,
        now: DateTime<Utc>,
        notices: &[NewNotice],
        resolve: &[&str],
    ) -> Result<(), StoreError> {
        let mut tx = self.pool.begin().await?;
        let row = sqlx::query("SELECT task_key, repo FROM pull_requests WHERE id = ?")
            .bind(id)
            .fetch_optional(&mut *tx)
            .await?;
        let Some(row) = row else { return Ok(()) };
        let key: String = row.try_get("task_key")?;
        let repo: String = row.try_get("repo")?;
        sqlx::query(
            "UPDATE pull_requests SET snapshot = ?, ci_failed_sha = ?, review_seen = ?,
                    issue_comment_seen = ?, review_comment_seen = ?, checked_at = ?, error = NULL,
                    error_kind = NULL
             WHERE id = ?",
        )
        .bind(serde_json::to_string(snapshot)?)
        .bind(&marks.ci_failed_sha)
        .bind(marks.review)
        .bind(marks.issue_comment)
        .bind(marks.review_comment)
        .bind(now.to_rfc3339())
        .bind(id)
        .execute(&mut *tx)
        .await?;
        resolve_kinds(&mut tx, &key, &repo, resolve).await?;
        for n in notices {
            insert_notice_in(&mut tx, &key, n).await?;
        }
        tx.commit().await?;
        Ok(())
    }

    /// Records the last check's problem as `(kind, message)` (`None` clears it).
    pub(crate) async fn set_pr_error(
        &self,
        id: i64,
        error: Option<(&str, &str)>,
    ) -> Result<(), StoreError> {
        sqlx::query("UPDATE pull_requests SET error_kind = ?, error = ? WHERE id = ?")
            .bind(error.map(|(kind, _)| kind))
            .bind(error.map(|(_, message)| message))
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Adds a notice unless the same (task, repo, kind, cause) exists; whether it was
    /// new.
    #[allow(dead_code)] // used by the API in the next packages
    pub(crate) async fn insert_notice(
        &self,
        key: &TaskKey,
        notice: &NewNotice,
    ) -> Result<bool, StoreError> {
        let mut conn = self.pool.acquire().await?;
        insert_notice_in(&mut conn, key.as_str(), notice).await
    }

    /// The notices still showing, oldest first.
    pub(crate) async fn open_notices(&self) -> Result<Vec<NoticeRow>, StoreError> {
        let rows = sqlx::query(&format!(
            "SELECT {NOTICE_COLUMNS} FROM pr_notices WHERE resolved_at IS NULL ORDER BY id"
        ))
        .fetch_all(&self.pool)
        .await?;
        rows.iter().map(notice_row).collect()
    }

    pub(crate) async fn notice(&self, id: i64) -> Result<Option<NoticeRow>, StoreError> {
        let row = sqlx::query(&format!(
            "SELECT {NOTICE_COLUMNS} FROM pr_notices WHERE id = ?"
        ))
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        row.as_ref().map(notice_row).transpose()
    }

    /// Resolved notices stay (so they are not raised again); whether this one was open.
    pub(crate) async fn resolve_notice(&self, id: i64) -> Result<bool, StoreError> {
        let result = sqlx::query(
            "UPDATE pr_notices SET resolved_at = ? WHERE id = ? AND resolved_at IS NULL",
        )
        .bind(Utc::now().to_rfc3339())
        .bind(id)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() > 0)
    }

    #[allow(dead_code)] // used by the archive in the next packages
    pub(crate) async fn resolve_notices_of(
        &self,
        key: &TaskKey,
        repo: &str,
        kinds: &[&str],
    ) -> Result<u64, StoreError> {
        let mut conn = self.pool.acquire().await?;
        resolve_kinds(&mut conn, key.as_str(), repo, kinds).await
    }
}

#[cfg(test)]
mod tests {
    use delilah_core::Task;
    use delilah_git::Workspace;

    use super::*;
    use crate::pr_types::{Outcome, PrState};
    use crate::prs::tests::{check, snapshot};

    async fn store_with(keys: &[&str]) -> Store {
        let store = Store::in_memory().await.unwrap();
        for k in keys {
            store
                .create_task(&Task::new(TaskKey::new(*k), "title", "feature"))
                .await
                .unwrap();
        }
        store
    }

    fn url(n: i64) -> String {
        format!("https://github.com/me/repo/pull/{n}")
    }

    async fn link(store: &Store, key: &str, repo: &str, n: i64) -> PrRow {
        store
            .link_pr(
                &TaskKey::new(key),
                &NewPr {
                    repo: repo.into(),
                    host: "github.com".into(),
                    owner_name: "me/repo".into(),
                    number: n,
                    url: url(n),
                    linked_by: "detected".into(),
                },
            )
            .await
            .unwrap()
    }

    fn notice(repo: &str, kind: &str, cause: &str) -> NewNotice {
        NewNotice {
            repo: repo.into(),
            kind: kind.into(),
            cause: cause.into(),
            text: "text".into(),
            detail: None,
        }
    }

    async fn table_count(store: &Store, table: &str) -> i64 {
        sqlx::query(&format!("SELECT COUNT(*) AS n FROM {table}"))
            .fetch_one(&store.pool)
            .await
            .unwrap()
            .try_get("n")
            .unwrap()
    }

    #[tokio::test]
    async fn link_pr_replaces_the_pr_of_the_same_repo_keeping_the_id() {
        let store = store_with(&["PROJ-1"]).await;
        let first = link(&store, "PROJ-1", ".", 1).await;
        let snap = snapshot(PrState::Open, vec![]);
        let marks = Marks {
            ci_failed_sha: Some("abc".into()),
            review: 3,
            issue_comment: 4,
            review_comment: 5,
        };
        store
            .save_check(first.id, &snap, &marks, Utc::now(), &[], &[])
            .await
            .unwrap();

        // The same URL again keeps what was seen.
        let again = link(&store, "PROJ-1", ".", 1).await;
        assert_eq!(again.id, first.id);
        assert_eq!(again.snapshot, Some(snap));
        assert_eq!(again.marks, marks);

        // Another URL starts clean.
        let second = link(&store, "PROJ-1", ".", 2).await;
        assert_eq!(second.id, first.id);
        assert_eq!(second.number, 2);
        assert_eq!(second.snapshot, None);
        assert_eq!(second.marks, Marks::default());
        assert_eq!(second.checked_at, None);
        assert_eq!(table_count(&store, "pull_requests").await, 1);

        link(&store, "PROJ-1", "svc", 3).await;
        assert_eq!(
            store.prs_of(&TaskKey::new("PROJ-1")).await.unwrap().len(),
            2
        );
        assert_eq!(store.pr(first.id).await.unwrap().unwrap().number, 2);
        assert!(store.pr(999).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn linking_another_pr_resolves_the_notices_of_the_old_one() {
        let store = store_with(&["PROJ-1"]).await;
        let key = TaskKey::new("PROJ-1");
        link(&store, "PROJ-1", ".", 1).await;
        link(&store, "PROJ-1", "svc", 5).await;
        for repo in [".", "svc"] {
            store
                .insert_notice(&key, &notice(repo, "ci-failed", "abc"))
                .await
                .unwrap();
        }

        // The same URL leaves them alone.
        link(&store, "PROJ-1", ".", 1).await;
        assert_eq!(store.open_notices().await.unwrap().len(), 2);

        // Another PR in "." resolves that repo's only.
        link(&store, "PROJ-1", ".", 2).await;
        let open = store.open_notices().await.unwrap();
        assert_eq!(open.len(), 1);
        assert_eq!(open[0].repo, "svc");
    }

    #[tokio::test]
    async fn set_pr_error_records_and_clears() {
        let store = store_with(&["PROJ-1"]).await;
        let pr = link(&store, "PROJ-1", ".", 1).await;
        store
            .set_pr_error(pr.id, Some(("network", "connection reset")))
            .await
            .unwrap();
        let row = store.pr(pr.id).await.unwrap().unwrap();
        assert_eq!(row.error.as_deref(), Some("connection reset"));
        assert_eq!(row.error_kind.as_deref(), Some("network"));
        store.set_pr_error(pr.id, None).await.unwrap();
        let row = store.pr(pr.id).await.unwrap().unwrap();
        assert_eq!((row.error, row.error_kind), (None, None));
    }

    #[tokio::test]
    async fn notice_reads_one_row_and_resolve_notice_closes_it_once() {
        let store = store_with(&["PROJ-1"]).await;
        let key = TaskKey::new("PROJ-1");
        assert!(store.notice(1).await.unwrap().is_none());
        store
            .insert_notice(&key, &notice(".", "comments", "9"))
            .await
            .unwrap();
        let id = store.open_notices().await.unwrap()[0].id;
        let n = store.notice(id).await.unwrap().unwrap();
        assert_eq!((n.kind.as_str(), n.cause.as_str()), ("comments", "9"));
        assert!(n.resolved_at.is_none());
        assert!(store.resolve_notice(id).await.unwrap());
        assert!(!store.resolve_notice(id).await.unwrap());
        assert!(!store.resolve_notice(id + 100).await.unwrap());
        assert!(
            store
                .notice(id)
                .await
                .unwrap()
                .unwrap()
                .resolved_at
                .is_some()
        );
    }

    #[tokio::test]
    async fn unlink_pr_deletes_resolves_and_remembers_the_url() {
        let store = store_with(&["PROJ-1"]).await;
        let key = TaskKey::new("PROJ-1");
        let pr = link(&store, "PROJ-1", ".", 1).await;
        store
            .insert_notice(&key, &notice(".", "ci-failed", "abc"))
            .await
            .unwrap();
        assert!(!store.is_unlinked(&key, &url(1)).await.unwrap());

        assert!(store.unlink_pr(pr.id).await.unwrap().is_some());
        assert!(store.pr(pr.id).await.unwrap().is_none());
        assert!(store.is_unlinked(&key, &url(1)).await.unwrap());
        assert!(store.open_notices().await.unwrap().is_empty());
        assert!(store.unlink_pr(pr.id).await.unwrap().is_none());

        // Linking it by hand again forgets the unlink.
        link(&store, "PROJ-1", ".", 1).await;
        assert!(!store.is_unlinked(&key, &url(1)).await.unwrap());
    }

    #[tokio::test]
    async fn the_same_notice_twice_keeps_one_row() {
        let store = store_with(&["PROJ-1"]).await;
        let key = TaskKey::new("PROJ-1");
        let n = notice(".", "ci-failed", "abc");
        assert!(store.insert_notice(&key, &n).await.unwrap());
        assert!(!store.insert_notice(&key, &n).await.unwrap());
        assert_eq!(table_count(&store, "pr_notices").await, 1);
        // Resolved notices stay, so the cause is not raised again.
        let id = store.open_notices().await.unwrap()[0].id;
        assert!(store.resolve_notice(id).await.unwrap());
        assert!(!store.resolve_notice(id).await.unwrap());
        assert!(!store.insert_notice(&key, &n).await.unwrap());
        assert!(store.open_notices().await.unwrap().is_empty());
        assert!(
            store
                .notice(id)
                .await
                .unwrap()
                .unwrap()
                .resolved_at
                .is_some()
        );
    }

    #[tokio::test]
    async fn save_check_writes_everything_at_once() {
        let store = store_with(&["PROJ-1"]).await;
        let key = TaskKey::new("PROJ-1");
        let pr = link(&store, "PROJ-1", ".", 1).await;
        store
            .insert_notice(&key, &notice(".", "ci-failed", "old"))
            .await
            .unwrap();
        store
            .insert_notice(&key, &notice(".", "comments", "9"))
            .await
            .unwrap();
        store
            .set_pr_error(pr.id, Some(("network", "network: down")))
            .await
            .unwrap();

        let snap = snapshot(PrState::Open, vec![check("test", Outcome::Failure)]);
        let marks = Marks {
            ci_failed_sha: Some("abc".into()),
            ..Marks::default()
        };
        let now = Utc::now();
        let mut raised = notice(".", "ci-failed", "abc");
        raised.detail = Some(serde_json::json!({"checks": ["test"]}));
        store
            .save_check(pr.id, &snap, &marks, now, &[raised], &["ci-failed"])
            .await
            .unwrap();

        let row = store.pr(pr.id).await.unwrap().unwrap();
        assert_eq!(row.snapshot, Some(snap));
        assert_eq!(row.marks, marks);
        assert_eq!(row.error, None);
        assert_eq!(row.error_kind, None);
        assert_eq!(row.checked_at.unwrap().timestamp(), now.timestamp());
        // The old ci-failed is resolved, the new one and the comments are open.
        let open = store.open_notices().await.unwrap();
        let causes: Vec<_> = open.iter().map(|n| n.cause.as_str()).collect();
        assert_eq!(causes, ["9", "abc"]);
        assert_eq!(
            open[1].detail,
            Some(serde_json::json!({"checks": ["test"]}))
        );

        assert_eq!(
            store
                .resolve_notices_of(&key, ".", &["comments"])
                .await
                .unwrap(),
            1
        );
        assert_eq!(store.open_notices().await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn deleting_the_task_cascades() {
        let store = store_with(&["PROJ-1", "PROJ-2"]).await;
        let key = TaskKey::new("PROJ-1");
        let pr = link(&store, "PROJ-1", ".", 1).await;
        link(&store, "PROJ-2", ".", 2).await;
        store
            .insert_notice(&key, &notice(".", "ci-failed", "a"))
            .await
            .unwrap();
        store.unlink_pr(pr.id).await.unwrap();
        link(&store, "PROJ-1", ".", 3).await;
        store
            .insert_notice(&key, &notice(".", "ci-failed", "b"))
            .await
            .unwrap();

        sqlx::query("DELETE FROM tasks WHERE key = 'PROJ-1'")
            .execute(&store.pool)
            .await
            .unwrap();
        assert_eq!(table_count(&store, "pull_requests").await, 1);
        assert_eq!(table_count(&store, "pr_notices").await, 0);
        assert_eq!(table_count(&store, "pr_unlinked").await, 0);
    }

    #[tokio::test]
    async fn open_prs_skips_closed_and_archived() {
        let store =
            store_with(&["NEW", "OPEN", "DRAFT", "MERGED", "CLOSED", "ARCH", "DONEWS"]).await;
        for (i, k) in ["NEW", "OPEN", "DRAFT", "MERGED", "CLOSED", "ARCH", "DONEWS"]
            .iter()
            .enumerate()
        {
            let pr = link(&store, k, ".", i as i64 + 1).await;
            let state = match *k {
                "MERGED" => Some(PrState::Merged),
                "CLOSED" => Some(PrState::Closed),
                "NEW" => None,
                _ => Some(PrState::Open),
            };
            if let Some(state) = state {
                let mut snap = snapshot(state, vec![]);
                snap.draft = *k == "DRAFT";
                store
                    .save_check(pr.id, &snap, &Marks::default(), Utc::now(), &[], &[])
                    .await
                    .unwrap();
            }
        }
        // Done tasks: archived without a workspace, still active with one.
        sqlx::query("UPDATE tasks SET status = ? WHERE key IN ('ARCH', 'DONEWS')")
            .bind(serde_json::to_string(&TaskStatus::Done).unwrap())
            .execute(&store.pool)
            .await
            .unwrap();
        store
            .save_workspace(
                &TaskKey::new("DONEWS"),
                "/tmp/parent".as_ref(),
                &Workspace {
                    path: "/tmp/ws".into(),
                    branch: "feature/DONEWS".into(),
                    kind: delilah_core::ProjectKind::Repo,
                    submodules: vec![],
                },
            )
            .await
            .unwrap();

        let keys: Vec<_> = store
            .open_prs()
            .await
            .unwrap()
            .into_iter()
            .map(|p| p.task_key)
            .collect();
        assert_eq!(keys, ["NEW", "OPEN", "DRAFT", "DONEWS"]);
    }
}
