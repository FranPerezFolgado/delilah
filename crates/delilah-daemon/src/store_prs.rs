//! Pull requests linked to tasks, what was last seen of them, and the notices they
//! raised (see `prs.rs`).

use chrono::{DateTime, Utc};
use delilah_core::{TaskKey, TaskStatus};
use serde_json::Value;
use sqlx::Row;
use sqlx::sqlite::SqliteRow;

use crate::autofix::PrAutofix;
use crate::pr_types::{Marks, PrState, Snapshot};
use crate::store::{Store, StoreError};

/// One PR linked to a task. A repo can have several (spec 015); a URL is linked once.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PrRow {
    pub id: i64,
    pub task_key: String,
    pub repo: String,
    pub host: String,
    pub owner_name: String,
    pub number: i64,
    pub url: String,
    /// `detected`, `opened`, `pasted` or `agent`.
    pub linked_by: String,
    pub snapshot: Option<Snapshot>,
    pub marks: Marks,
    pub checked_at: Option<DateTime<Utc>>,
    pub error: Option<String>,
    /// The kind of problem behind `error` (see `AccessProblem::kind`).
    pub error_kind: Option<String>,
    pub created_at: DateTime<Utc>,
    /// CI autofix state; `None` until autofix first acts on the PR.
    pub autofix: Option<PrAutofix>,
    /// Id of the PR that replaced this closed one; `None` = a counted PR.
    pub replaced_by: Option<i64>,
}

/// The PRs that count (not replaced by a newer one).
pub(crate) fn counted(rows: &[PrRow]) -> impl Iterator<Item = &PrRow> {
    rows.iter().filter(|r| r.replaced_by.is_none())
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
    /// The PR the notice is about; empty for task-level notices.
    pub pr_url: String,
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
     ci_failed_sha, review_seen, issue_comment_seen, review_comment_seen, checked_at, error, error_kind, created_at, autofix, replaced_by";
/// The `pr_notices.kind` of "the branch started from a base that could not be
/// refreshed". It belongs to the task's start, not to a PR, so PR changes leave it.
pub(crate) const KIND_BASE_STALE: &str = "base-stale";
/// A non-primary session finished a turn (`detail.session` names it). A plain aviso.
pub(crate) const KIND_TURN_DONE: &str = "turn-done";
/// A PR was opened or linked for the task. A plain aviso, once per PR URL.
pub(crate) const KIND_PR_OPENED: &str = "pr-opened";

/// The notices about one PR: they carry its URL and are resolved with it. Everything
/// else is task-level (`pr_url` empty).
pub(crate) const PR_NOTICE_KINDS: [&str; 5] = [
    "ci-failed",
    "changes-requested",
    "comments",
    KIND_PR_OPENED,
    "autofix-stopped",
];

const NOTICE_COLUMNS: &str =
    "id, task_key, repo, kind, cause, text, detail, created_at, resolved_at, pr_url";

fn time(raw: &str) -> Result<DateTime<Utc>, StoreError> {
    DateTime::parse_from_rfc3339(raw)
        .map(|t| t.with_timezone(&Utc))
        .map_err(|e| StoreError::Corrupt(format!("bad time `{raw}`: {e}")))
}

fn opt_time(raw: Option<String>) -> Result<Option<DateTime<Utc>>, StoreError> {
    raw.as_deref().map(time).transpose()
}

/// Reads the stored autofix state; a value that does not parse (a newer or damaged
/// row) reads as `None` so it cannot take the PR list down.
fn lenient_autofix(pr_id: i64, raw: Option<String>) -> Option<PrAutofix> {
    match serde_json::from_str(&raw?) {
        Ok(autofix) => Some(autofix),
        Err(e) => {
            tracing::warn!(pr = pr_id, error = %e, "unreadable autofix state, treating it as none");
            None
        }
    }
}

fn pr_row(row: &SqliteRow) -> Result<PrRow, StoreError> {
    let snapshot: Option<String> = row.try_get("snapshot")?;
    let autofix: Option<String> = row.try_get("autofix")?;
    let id: i64 = row.try_get("id")?;
    Ok(PrRow {
        id,
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
        autofix: lenient_autofix(id, autofix),
        replaced_by: row.try_get("replaced_by")?,
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
        pr_url: row.try_get("pr_url")?,
    })
}

/// Resolves the open notices of `kinds` for a task's repo (of one PR when `pr_url` is
/// given), inside a transaction.
async fn resolve_kinds(
    tx: &mut sqlx::SqliteConnection,
    key: &str,
    repo: &str,
    pr_url: Option<&str>,
    kinds: &[&str],
) -> Result<u64, StoreError> {
    let mut done = 0;
    for kind in kinds {
        done += sqlx::query(
            "UPDATE pr_notices SET resolved_at = ?
             WHERE task_key = ? AND repo = ? AND kind = ? AND resolved_at IS NULL
               AND (? IS NULL OR pr_url = ?)",
        )
        .bind(Utc::now().to_rfc3339())
        .bind(key)
        .bind(repo)
        .bind(kind)
        .bind(pr_url)
        .bind(pr_url)
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
    pr_url: &str,
) -> Result<bool, StoreError> {
    let detail = n.detail.as_ref().map(serde_json::to_string).transpose()?;
    let result = sqlx::query(
        "INSERT OR IGNORE INTO pr_notices (task_key, repo, kind, cause, text, detail, created_at, pr_url)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(key)
    .bind(&n.repo)
    .bind(&n.kind)
    .bind(&n.cause)
    .bind(&n.text)
    .bind(detail)
    .bind(Utc::now().to_rfc3339())
    .bind(pr_url)
    .execute(&mut *tx)
    .await?;
    Ok(result.rows_affected() > 0)
}

/// Marks the closed PRs of `new`'s task, repo and head branch as replaced by it and
/// resolves their open notices.
async fn replace_closed(
    tx: &mut sqlx::SqliteConnection,
    new: &PrRow,
    branch: &str,
) -> Result<(), StoreError> {
    let rows = sqlx::query(&format!(
        "SELECT {PR_COLUMNS} FROM pull_requests
         WHERE task_key = ? AND repo = ? AND id != ? AND replaced_by IS NULL"
    ))
    .bind(&new.task_key)
    .bind(&new.repo)
    .bind(new.id)
    .fetch_all(&mut *tx)
    .await?;
    for old in rows.iter().map(pr_row).collect::<Result<Vec<_>, _>>()? {
        let closed_on_branch = old.snapshot.as_ref().is_some_and(|s| {
            s.state == PrState::Closed && s.head_branch.as_deref() == Some(branch)
        });
        if !closed_on_branch {
            continue;
        }
        sqlx::query("UPDATE pull_requests SET replaced_by = ? WHERE id = ?")
            .bind(new.id)
            .bind(old.id)
            .execute(&mut *tx)
            .await?;
        sqlx::query(
            "UPDATE pr_notices SET resolved_at = ?
             WHERE task_key = ? AND pr_url = ? AND resolved_at IS NULL",
        )
        .bind(Utc::now().to_rfc3339())
        .bind(&old.task_key)
        .bind(&old.url)
        .execute(&mut *tx)
        .await?;
    }
    Ok(())
}

impl Store {
    /// Compare-and-set of the PR's autofix state: writes `next` only if the stored
    /// state equals `expected` (compared by value, not as text: the migration's JSON
    /// differs from serde's). `false` when it differs, leaving the row as it was.
    pub(crate) async fn update_pr_autofix(
        &self,
        pr_id: i64,
        expected: Option<&PrAutofix>,
        next: Option<&PrAutofix>,
    ) -> Result<bool, StoreError> {
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let stored: Option<Option<String>> =
            sqlx::query_scalar("SELECT autofix FROM pull_requests WHERE id = ?")
                .bind(pr_id)
                .fetch_optional(&mut *tx)
                .await?;
        let Some(stored) = stored else {
            return Ok(false);
        };
        let current = lenient_autofix(pr_id, stored);
        if current.as_ref() != expected {
            return Ok(false);
        }
        sqlx::query("UPDATE pull_requests SET autofix = ? WHERE id = ?")
            .bind(next.map(serde_json::to_string).transpose()?)
            .bind(pr_id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(true)
    }

    /// Links a PR to a task. A repo can hold several PRs: the row is keyed by URL, so
    /// linking the same URL again keeps what was seen of it and other PRs (and their
    /// notices) are left alone.
    #[cfg(test)]
    pub(crate) async fn link_pr(&self, key: &TaskKey, pr: &NewPr) -> Result<PrRow, StoreError> {
        self.link_pr_on_branch(key, pr, None).await
    }

    /// [`Store::link_pr`] for a PR whose head branch is known: a closed (not merged) PR
    /// of the task with the same repo and head branch is marked `replaced_by` this one
    /// and its notices are resolved (FR-009). Without a branch nothing is replaced.
    pub(crate) async fn link_pr_on_branch(
        &self,
        key: &TaskKey,
        pr: &NewPr,
        head_branch: Option<&str>,
    ) -> Result<PrRow, StoreError> {
        let mut tx = self.pool.begin().await?;
        let exists: Option<i64> =
            sqlx::query_scalar("SELECT id FROM pull_requests WHERE task_key = ? AND url = ?")
                .bind(key.as_str())
                .bind(&pr.url)
                .fetch_optional(&mut *tx)
                .await?;
        let is_new = exists.is_none();
        // Never INSERT OR REPLACE: it would change the id.
        sqlx::query(
            "INSERT INTO pull_requests (task_key, repo, host, owner_name, number, url, linked_by, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT (task_key, url) DO UPDATE SET
               repo = excluded.repo,
               host = excluded.host,
               owner_name = excluded.owner_name,
               number = excluded.number,
               linked_by = excluded.linked_by",
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
        // Opening, pasting and detecting all come through here. The URL as the cause
        // makes it once per PR: unlinking and linking it again is not news.
        if is_new {
            let opened = NewNotice {
                repo: pr.repo.clone(),
                kind: KIND_PR_OPENED.into(),
                cause: pr.url.clone(),
                text: format!("{}#{}", pr.owner_name, pr.number),
                detail: None,
            };
            insert_notice_in(&mut tx, key.as_str(), &opened, &pr.url).await?;
        }
        let row = sqlx::query(&format!(
            "SELECT {PR_COLUMNS} FROM pull_requests WHERE task_key = ? AND url = ?"
        ))
        .bind(key.as_str())
        .bind(&pr.url)
        .fetch_one(&mut *tx)
        .await?;
        let row = pr_row(&row)?;
        if let Some(branch) = head_branch {
            replace_closed(&mut tx, &row, branch).await?;
        }
        tx.commit().await?;
        Ok(row)
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
        // Only this PR's notices go; task-level ones (stale base...) are not about it.
        sqlx::query(
            "UPDATE pr_notices SET resolved_at = ?
             WHERE task_key = ? AND pr_url = ? AND kind != ? AND resolved_at IS NULL",
        )
        .bind(Utc::now().to_rfc3339())
        .bind(&row.task_key)
        .bind(&row.url)
        .bind(KIND_BASE_STALE)
        .execute(&mut *tx)
        .await?;
        // A closed PR this one replaced counts again.
        sqlx::query("UPDATE pull_requests SET replaced_by = NULL WHERE replaced_by = ?")
            .bind(id)
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
        let row = sqlx::query("SELECT task_key, repo, url FROM pull_requests WHERE id = ?")
            .bind(id)
            .fetch_optional(&mut *tx)
            .await?;
        let Some(row) = row else { return Ok(()) };
        let key: String = row.try_get("task_key")?;
        let repo: String = row.try_get("repo")?;
        let pr_url: String = row.try_get("url")?;
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
        resolve_kinds(&mut tx, &key, &repo, Some(&pr_url), resolve).await?;
        for n in notices {
            insert_notice_in(&mut tx, &key, n, &pr_url).await?;
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

    /// Adds a notice about one PR (`pr_url`; empty for a task-level one) unless the same
    /// (task, repo, pr_url, kind, cause) exists; whether it was new.
    pub(crate) async fn insert_notice_for(
        &self,
        key: &TaskKey,
        notice: &NewNotice,
        pr_url: &str,
    ) -> Result<bool, StoreError> {
        let mut conn = self.pool.acquire().await?;
        insert_notice_in(&mut conn, key.as_str(), notice, pr_url).await
    }

    /// Adds a task-level notice (no PR: `pr_url` empty); whether it was new. A notice
    /// about one PR (`PR_NOTICE_KINDS`) goes through [`Store::insert_notice_for`] and is
    /// refused here: with no URL it could never be resolved with its PR.
    pub(crate) async fn insert_task_notice(
        &self,
        key: &TaskKey,
        notice: &NewNotice,
    ) -> Result<bool, StoreError> {
        if PR_NOTICE_KINDS.contains(&notice.kind.as_str()) {
            return Err(StoreError::Corrupt(format!(
                "`{}` notices are about a PR: use insert_notice_for",
                notice.kind
            )));
        }
        self.insert_notice_for(key, notice, "").await
    }

    /// The notices still showing that ask for something, oldest first. The avisos
    /// (`turn-done`, `pr-opened`) are apart: see [`Store::open_avisos`].
    pub(crate) async fn open_notices(&self) -> Result<Vec<NoticeRow>, StoreError> {
        let rows = sqlx::query(&format!(
            "SELECT {NOTICE_COLUMNS} FROM pr_notices
             WHERE resolved_at IS NULL AND kind NOT IN (?, ?) ORDER BY id"
        ))
        .bind(KIND_TURN_DONE)
        .bind(KIND_PR_OPENED)
        .fetch_all(&self.pool)
        .await?;
        rows.iter().map(notice_row).collect()
    }

    /// The avisos still showing (a session ended a turn, a PR was opened), oldest first.
    pub(crate) async fn open_avisos(&self) -> Result<Vec<NoticeRow>, StoreError> {
        let rows = sqlx::query(&format!(
            "SELECT {NOTICE_COLUMNS} FROM pr_notices
             WHERE resolved_at IS NULL AND kind IN (?, ?) ORDER BY id"
        ))
        .bind(KIND_TURN_DONE)
        .bind(KIND_PR_OPENED)
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

    /// Resolves every open notice of a task, whatever its repo or kind; how many.
    pub(crate) async fn resolve_all_notices_of_task(
        &self,
        key: &TaskKey,
    ) -> Result<u64, StoreError> {
        let result = sqlx::query(
            "UPDATE pr_notices SET resolved_at = ? WHERE task_key = ? AND resolved_at IS NULL",
        )
        .bind(Utc::now().to_rfc3339())
        .bind(key.as_str())
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected())
    }

    /// Resolves the open notices of `kinds` for a repo, whatever PR they are about (the
    /// task-level ones carry no PR).
    pub(crate) async fn resolve_notices_of(
        &self,
        key: &TaskKey,
        repo: &str,
        kinds: &[&str],
    ) -> Result<u64, StoreError> {
        let mut conn = self.pool.acquire().await?;
        resolve_kinds(&mut conn, key.as_str(), repo, None, kinds).await
    }

    /// Like [`Store::resolve_notices_of`] but only for the notices of one PR.
    pub(crate) async fn resolve_notices_of_pr(
        &self,
        key: &TaskKey,
        repo: &str,
        pr_url: &str,
        kinds: &[&str],
    ) -> Result<u64, StoreError> {
        let mut conn = self.pool.acquire().await?;
        resolve_kinds(&mut conn, key.as_str(), repo, Some(pr_url), kinds).await
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
                .create_task(&Task::new(TaskKey::new(*k), "title", "project"))
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

    #[tokio::test]
    async fn a_pr_linked_for_the_first_time_raises_one_pr_opened_notice_per_url() {
        let store = store_with(&["PROJ-1"]).await;
        let opened = |store: &Store| {
            let store = store.clone();
            async move { store.open_avisos().await.unwrap() }
        };
        link(&store, "PROJ-1", ".", 1).await;
        link(&store, "PROJ-1", ".", 1).await;
        let first = opened(&store).await;
        assert_eq!(first.len(), 1);
        assert_eq!(first[0].cause, url(1));
        // Another PR in the repo is another aviso, about its own PR.
        link(&store, "PROJ-1", ".", 2).await;
        let now = opened(&store).await;
        assert_eq!(now.len(), 2);
        assert_eq!(now[1].cause, url(2));
        assert_eq!(now[1].pr_url, url(2));
    }

    fn fixing(sha: &str, attempts: u32) -> PrAutofix {
        PrAutofix {
            state: crate::autofix::State::Fixing,
            attempts,
            sha: Some(sha.into()),
            session: Some("s1".into()),
            sent_at: Some("2026-01-01T00:00:00Z".into()),
            reason: None,
        }
    }

    #[tokio::test]
    async fn update_pr_autofix_is_a_compare_and_set() {
        let store = store_with(&["PROJ-1"]).await;
        let pr = link(&store, "PROJ-1", ".", 1).await;
        assert_eq!(pr.autofix, None);
        let a = fixing("aaa", 1);
        let b = fixing("bbb", 2);
        // None -> Some on a NULL row.
        assert!(
            store
                .update_pr_autofix(pr.id, None, Some(&a))
                .await
                .unwrap()
        );
        // A stale expected changes nothing.
        assert!(
            !store
                .update_pr_autofix(pr.id, None, Some(&b))
                .await
                .unwrap()
        );
        assert!(
            !store
                .update_pr_autofix(pr.id, Some(&b), Some(&b))
                .await
                .unwrap()
        );
        assert_eq!(
            store.pr(pr.id).await.unwrap().unwrap().autofix,
            Some(a.clone())
        );
        assert!(
            store
                .update_pr_autofix(pr.id, Some(&a), Some(&b))
                .await
                .unwrap()
        );
        assert!(
            store
                .update_pr_autofix(pr.id, Some(&b), None)
                .await
                .unwrap()
        );
        assert_eq!(store.pr(pr.id).await.unwrap().unwrap().autofix, None);
        assert!(!store.update_pr_autofix(9999, None, Some(&a)).await.unwrap());
    }

    #[tokio::test]
    async fn update_pr_autofix_matches_the_migration_backfill_shape() {
        let store = store_with(&["PROJ-1"]).await;
        let pr = link(&store, "PROJ-1", ".", 1).await;
        sqlx::query(
            "UPDATE pull_requests SET autofix = '{\"sha\": null,  \"attempts\":0,\"state\":\"watching\"}' WHERE id = ?",
        )
        .bind(pr.id)
        .execute(&store.pool)
        .await
        .unwrap();
        let got = store.pr(pr.id).await.unwrap().unwrap().autofix.unwrap();
        assert_eq!(got.state, crate::autofix::State::Watching);
        assert_eq!(
            (got.attempts, got.sha.clone(), got.session.clone()),
            (0, None, None)
        );
        let next = fixing("ccc", 1);
        assert!(
            store
                .update_pr_autofix(pr.id, Some(&got), Some(&next))
                .await
                .unwrap()
        );
        assert_eq!(store.pr(pr.id).await.unwrap().unwrap().autofix, Some(next));
    }

    #[tokio::test]
    async fn concurrent_autofix_cas_on_a_file_store_has_one_winner_and_no_error() {
        let dir = std::env::temp_dir().join(format!("delilah-prs-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let store = Store::open(&dir.join("delilah.db")).await.unwrap();
        store
            .create_task(&Task::new(TaskKey::new("PROJ-1"), "title", "feature"))
            .await
            .unwrap();
        let pr = link(&store, "PROJ-1", ".", 1).await;
        for round in 0..20 {
            let (a, b) = (fixing("aaa", round), fixing("bbb", round));
            let (x, y) = tokio::join!(
                store.update_pr_autofix(pr.id, None, Some(&a)),
                store.update_pr_autofix(pr.id, None, Some(&b)),
            );
            let (x, y) = (x.expect("no error"), y.expect("no error"));
            assert!(x ^ y, "exactly one winner (round {round})");
            let now = store.pr(pr.id).await.unwrap().unwrap().autofix;
            assert!(
                store
                    .update_pr_autofix(pr.id, now.as_ref(), None)
                    .await
                    .unwrap()
            );
        }
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[tokio::test]
    async fn relinking_keeps_autofix_for_the_same_url_and_drops_it_for_another() {
        let store = store_with(&["PROJ-1"]).await;
        let pr = link(&store, "PROJ-1", ".", 1).await;
        let a = fixing("aaa", 1);
        assert!(
            store
                .update_pr_autofix(pr.id, None, Some(&a))
                .await
                .unwrap()
        );
        let same = link(&store, "PROJ-1", ".", 1).await;
        assert_eq!(same.autofix, Some(a));
        let other = link(&store, "PROJ-1", ".", 2).await;
        assert_ne!(other.id, pr.id);
        assert_eq!(other.autofix, None);
        assert_eq!(
            store.pr(pr.id).await.unwrap().unwrap().autofix,
            Some(fixing("aaa", 1))
        );
    }

    #[tokio::test]
    async fn unreadable_autofix_reads_as_none_and_can_be_overwritten() {
        let store = store_with(&["PROJ-1", "PROJ-2"]).await;
        let pr = link(&store, "PROJ-1", ".", 1).await;
        let other = link(&store, "PROJ-2", ".", 2).await;
        for bad in ["not json", "{\"state\":\"from-the-future\",\"attempts\":0}"] {
            sqlx::query("UPDATE pull_requests SET autofix = ? WHERE id = ?")
                .bind(bad)
                .bind(pr.id)
                .execute(&store.pool)
                .await
                .unwrap();
            assert_eq!(store.pr(pr.id).await.unwrap().unwrap().autofix, None);
            assert_eq!(
                store.prs_of(&TaskKey::new("PROJ-1")).await.unwrap().len(),
                1
            );
            assert!(!store.open_prs().await.unwrap().is_empty());
            let next = fixing("ddd", 1);
            assert!(
                store
                    .update_pr_autofix(pr.id, None, Some(&next))
                    .await
                    .unwrap()
            );
            assert_eq!(store.pr(pr.id).await.unwrap().unwrap().autofix, Some(next));
        }
        assert_eq!(store.pr(other.id).await.unwrap().unwrap().autofix, None);
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
    async fn two_urls_in_one_repo_keep_two_rows_and_the_same_url_keeps_one() {
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

        // The same URL again keeps the row, its id, marks and snapshot.
        let again = link(&store, "PROJ-1", ".", 1).await;
        assert_eq!(again.id, first.id);
        assert_eq!(again.snapshot, Some(snap.clone()));
        assert_eq!(again.marks, marks);
        assert_eq!(table_count(&store, "pull_requests").await, 1);

        // Another URL in the same repo is another row; the first is untouched.
        let second = link(&store, "PROJ-1", ".", 2).await;
        assert_ne!(second.id, first.id);
        assert_eq!(second.snapshot, None);
        assert_eq!(second.marks, Marks::default());
        assert_eq!(table_count(&store, "pull_requests").await, 2);
        let kept = store.pr(first.id).await.unwrap().unwrap();
        assert_eq!(
            (kept.number, kept.snapshot, kept.marks),
            (1, Some(snap), marks)
        );

        link(&store, "PROJ-1", "svc", 3).await;
        assert_eq!(
            store.prs_of(&TaskKey::new("PROJ-1")).await.unwrap().len(),
            3
        );
        assert!(store.pr(999).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn linking_a_new_pr_leaves_the_notices_of_the_others_alone() {
        let store = store_with(&["PROJ-1"]).await;
        let key = TaskKey::new("PROJ-1");
        link(&store, "PROJ-1", ".", 1).await;
        link(&store, "PROJ-1", "svc", 5).await;
        store
            .insert_notice_for(&key, &notice(".", "ci-failed", "abc"), &url(1))
            .await
            .unwrap();
        store
            .insert_notice_for(&key, &notice("svc", "ci-failed", "abc"), &url(5))
            .await
            .unwrap();
        store
            .insert_task_notice(&key, &notice(".", KIND_BASE_STALE, "feature/x"))
            .await
            .unwrap();

        // The same URL, a new PR in "." and a new PR elsewhere: nothing is resolved.
        link(&store, "PROJ-1", ".", 1).await;
        link(&store, "PROJ-1", ".", 2).await;
        link(&store, "PROJ-1", "svc", 6).await;
        assert_eq!(store.open_notices().await.unwrap().len(), 3);

        // The same cause on two PRs of one repo are two notices.
        assert!(
            store
                .insert_notice_for(&key, &notice(".", "ci-failed", "abc"), &url(2))
                .await
                .unwrap()
        );
        let open = store.open_notices().await.unwrap();
        let of_pr: Vec<_> = open
            .iter()
            .filter(|n| n.kind == "ci-failed")
            .map(|n| n.pr_url.as_str())
            .collect();
        assert_eq!(of_pr, [url(1), url(5), url(2)]);
    }

    #[tokio::test]
    async fn unlink_pr_clears_replaced_by_pointing_at_it_and_only_its_notices() {
        let store = store_with(&["PROJ-1"]).await;
        let key = TaskKey::new("PROJ-1");
        let old = link(&store, "PROJ-1", ".", 1).await;
        let newer = link(&store, "PROJ-1", ".", 2).await;
        let sibling = link(&store, "PROJ-1", ".", 3).await;
        sqlx::query("UPDATE pull_requests SET replaced_by = ? WHERE id = ?")
            .bind(newer.id)
            .bind(old.id)
            .execute(&store.pool)
            .await
            .unwrap();
        let rows = store.prs_of(&key).await.unwrap();
        assert_eq!(
            counted(&rows).map(|r| r.id).collect::<Vec<_>>(),
            [newer.id, sibling.id]
        );
        store
            .insert_notice_for(&key, &notice(".", "ci-failed", "a"), &url(2))
            .await
            .unwrap();
        store
            .insert_notice_for(&key, &notice(".", "ci-failed", "a"), &url(3))
            .await
            .unwrap();

        store.unlink_pr(newer.id).await.unwrap();
        assert_eq!(store.pr(old.id).await.unwrap().unwrap().replaced_by, None);
        let rows = store.prs_of(&key).await.unwrap();
        assert_eq!(counted(&rows).count(), 2);
        let open: Vec<_> = store
            .open_notices()
            .await
            .unwrap()
            .into_iter()
            .map(|n| n.pr_url)
            .collect();
        assert_eq!(open, [url(3)]);
        assert!(store.pr(sibling.id).await.unwrap().is_some());
    }

    /// Runs the migrations up to `version` on a fresh in-memory database.
    async fn db_at(version: i64) -> sqlx::SqlitePool {
        use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
        use std::str::FromStr;
        let options = SqliteConnectOptions::from_str("sqlite::memory:")
            .unwrap()
            .foreign_keys(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await
            .unwrap();
        let mut m = sqlx::migrate!("./migrations");
        m.migrations = m
            .migrations
            .iter()
            .filter(|m| m.version <= version)
            .cloned()
            .collect::<Vec<_>>()
            .into();
        m.run(&pool).await.unwrap();
        pool
    }

    #[tokio::test]
    async fn migration_0027_keeps_prs_and_notices_and_backfills_pr_url() {
        let pool = db_at(26).await;
        sqlx::query(
            "INSERT INTO tasks (key, title, project, workflow, phase, status, created_at, updated_at)
             VALUES ('OLD-1', 'Old', NULL, 'feature', 'backlog', '{\"type\":\"queued\"}', 'x', 'x')",
        )
        .execute(&pool)
        .await
        .unwrap();
        let autofix = r#"{"state":"fixing","attempts":2,"sha":"abc"}"#;
        let snap = serde_json::to_string(&snapshot(PrState::Open, vec![])).unwrap();
        let snap = snap.as_str();
        for (id, repo, n) in [(7, ".", 1), (9, "svc", 2)] {
            sqlx::query(
                "INSERT INTO pull_requests (id, task_key, repo, host, owner_name, number, url, linked_by,
                   snapshot, ci_failed_sha, review_seen, issue_comment_seen, review_comment_seen,
                   checked_at, error, error_kind, created_at, autofix)
                 VALUES (?, 'OLD-1', ?, 'github.com', 'me/repo', ?, ?, 'pasted', ?, 'abc', 3, 4, 5,
                   '2026-01-02T00:00:00Z', 'boom', 'network', '2026-01-01T00:00:00Z', ?)",
            )
            .bind(id)
            .bind(repo)
            .bind(n)
            .bind(url(n))
            .bind(snap)
            .bind(autofix)
            .execute(&pool)
            .await
            .unwrap();
        }
        // (id, repo, kind, cause, resolved_at)
        let notices = [
            (1, ".", "ci-failed", "abc", None),
            (2, "gone", "comments", "9", None),
            (3, ".", "archive-blocked", "merge", None),
            (6, ".", "autofix-stopped", "abc:limit", None),
            (4, "svc", "pr-opened", "u", Some("2026-01-03T00:00:00Z")),
            (
                5,
                "gone",
                "changes-requested",
                "r",
                Some("2026-01-03T00:00:00Z"),
            ),
        ];
        for (id, repo, kind, cause, resolved) in notices {
            sqlx::query(
                "INSERT INTO pr_notices (id, task_key, repo, kind, cause, text, detail, created_at, resolved_at)
                 VALUES (?, 'OLD-1', ?, ?, ?, 'text', '{\"k\":1}', '2026-01-01T00:00:00Z', ?)",
            )
            .bind(id)
            .bind(repo)
            .bind(kind)
            .bind(cause)
            .bind(resolved)
            .execute(&pool)
            .await
            .unwrap();
        }

        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        let store = Store { pool };
        let prs = store.prs_of(&TaskKey::new("OLD-1")).await.unwrap();
        assert_eq!(prs.iter().map(|p| p.id).collect::<Vec<_>>(), [7, 9]);
        let p = &prs[0];
        assert_eq!(
            (p.repo.as_str(), p.number, p.linked_by.as_str()),
            (".", 1, "pasted")
        );
        assert_eq!(p.url, url(1));
        assert_eq!(
            p.marks,
            Marks {
                ci_failed_sha: Some("abc".into()),
                review: 3,
                issue_comment: 4,
                review_comment: 5
            }
        );
        assert_eq!(
            p.checked_at.unwrap().to_rfc3339(),
            "2026-01-02T00:00:00+00:00"
        );
        assert_eq!(
            (p.error.as_deref(), p.error_kind.as_deref()),
            (Some("boom"), Some("network"))
        );
        assert_eq!(p.autofix.as_ref().unwrap().attempts, 2);
        assert_eq!(p.replaced_by, None);
        let raw: String = sqlx::query_scalar("SELECT snapshot FROM pull_requests WHERE id = 7")
            .fetch_one(&store.pool)
            .await
            .unwrap();
        assert_eq!(raw, snap);

        let all = |id: i64| {
            let store = store.clone();
            async move { store.notice(id).await.unwrap().unwrap() }
        };
        let n1 = all(1).await;
        assert_eq!(
            (n1.pr_url.as_str(), n1.text.as_str()),
            (url(1).as_str(), "text")
        );
        assert_eq!(n1.detail, Some(serde_json::json!({"k": 1})));
        assert!(n1.resolved_at.is_none());
        // PR notice whose PR is gone: resolved, no url.
        let n2 = all(2).await;
        assert_eq!(n2.pr_url, "");
        assert!(n2.resolved_at.is_some());
        // Task-level notice: untouched, no url.
        let n3 = all(3).await;
        assert_eq!(n3.pr_url, "");
        assert!(n3.resolved_at.is_none());
        // A legacy autofix-stopped notice takes its PR's url like the other PR kinds.
        let n6 = all(6).await;
        assert_eq!(n6.pr_url, url(1));
        assert!(n6.resolved_at.is_none());
        // Already resolved ones keep their time.
        assert_eq!(all(4).await.pr_url, url(2));
        assert_eq!(
            all(4).await.resolved_at.unwrap().to_rfc3339(),
            "2026-01-03T00:00:00+00:00"
        );
        assert_eq!(
            all(5).await.resolved_at.unwrap().to_rfc3339(),
            "2026-01-03T00:00:00+00:00"
        );

        // The new keys are in force: two PRs per repo, but one row per URL.
        link(&store, "OLD-1", ".", 10).await;
        link(&store, "OLD-1", ".", 1).await;
        assert_eq!(table_count(&store, "pull_requests").await, 3);
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
            .insert_notice_for(&key, &notice(".", "comments", "9"), &url(1))
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
            .insert_notice_for(&key, &notice(".", "ci-failed", "abc"), &pr.url)
            .await
            .unwrap();
        store
            .insert_task_notice(&key, &notice(".", KIND_BASE_STALE, "feature/x"))
            .await
            .unwrap();
        assert!(!store.is_unlinked(&key, &url(1)).await.unwrap());

        assert!(store.unlink_pr(pr.id).await.unwrap().is_some());
        assert!(store.pr(pr.id).await.unwrap().is_none());
        assert!(store.is_unlinked(&key, &url(1)).await.unwrap());
        // Only the PR's own notices go: the stale-base warning stays.
        let open = store.open_notices().await.unwrap();
        assert_eq!(open.len(), 1, "{open:?}");
        assert_eq!(open[0].kind, KIND_BASE_STALE);
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
        assert!(store.insert_notice_for(&key, &n, &url(1)).await.unwrap());
        assert!(!store.insert_notice_for(&key, &n, &url(1)).await.unwrap());
        assert_eq!(table_count(&store, "pr_notices").await, 1);
        // Resolved notices stay, so the cause is not raised again.
        let id = store.open_notices().await.unwrap()[0].id;
        assert!(store.resolve_notice(id).await.unwrap());
        assert!(!store.resolve_notice(id).await.unwrap());
        assert!(!store.insert_notice_for(&key, &n, &url(1)).await.unwrap());
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
            .insert_notice_for(&key, &notice(".", "ci-failed", "old"), &pr.url)
            .await
            .unwrap();
        store
            .insert_notice_for(&key, &notice(".", "comments", "9"), &pr.url)
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
            .insert_notice_for(&key, &notice(".", "ci-failed", "a"), &url(1))
            .await
            .unwrap();
        store.unlink_pr(pr.id).await.unwrap();
        link(&store, "PROJ-1", ".", 3).await;
        store
            .insert_notice_for(&key, &notice(".", "ci-failed", "b"), &url(1))
            .await
            .unwrap();

        sqlx::query("DELETE FROM tasks WHERE key = 'PROJ-1'")
            .execute(&store.pool)
            .await
            .unwrap();
        assert_eq!(table_count(&store, "pull_requests").await, 1);
        // PROJ-2 keeps its own pr-opened aviso.
        assert_eq!(table_count(&store, "pr_notices").await, 1);
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

    async fn link_on(store: &Store, key: &str, n: i64, branch: Option<&str>) -> PrRow {
        let pr = NewPr {
            repo: ".".into(),
            host: "github.com".into(),
            owner_name: "me/repo".into(),
            number: n,
            url: url(n),
            linked_by: "detected".into(),
        };
        store
            .link_pr_on_branch(&TaskKey::new(key), &pr, branch)
            .await
            .unwrap()
    }

    async fn checked(store: &Store, id: i64, state: PrState, branch: &str) {
        let mut snap = snapshot(state, vec![]);
        snap.head_branch = Some(branch.into());
        store
            .save_check(id, &snap, &Marks::default(), Utc::now(), &[], &[])
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn a_new_pr_on_the_branch_of_a_closed_one_replaces_it_and_resolves_its_notices() {
        let store = store_with(&["PROJ-1"]).await;
        let key = TaskKey::new("PROJ-1");
        let old = link_on(&store, "PROJ-1", 1, None).await;
        checked(&store, old.id, PrState::Closed, "feat/x").await;
        store
            .insert_notice_for(&key, &notice(".", "ci-failed", "a"), &old.url)
            .await
            .unwrap();
        let new = link_on(&store, "PROJ-1", 2, Some("feat/x")).await;
        let old = store.pr(old.id).await.unwrap().unwrap();
        assert_eq!(old.replaced_by, Some(new.id));
        assert_eq!(new.replaced_by, None);
        let open: Vec<_> = store.open_notices().await.unwrap();
        assert!(open.iter().all(|n| n.pr_url != old.url), "{open:?}");
        assert_eq!(counted(&store.prs_of(&key).await.unwrap()).count(), 1);
    }

    #[tokio::test]
    async fn open_merged_other_branch_or_other_repo_prs_are_never_replaced() {
        let store = store_with(&["PROJ-1"]).await;
        let open = link_on(&store, "PROJ-1", 1, None).await;
        checked(&store, open.id, PrState::Open, "feat/x").await;
        let merged = link_on(&store, "PROJ-1", 2, None).await;
        checked(&store, merged.id, PrState::Merged, "feat/x").await;
        let other = link_on(&store, "PROJ-1", 3, None).await;
        checked(&store, other.id, PrState::Closed, "feat/other").await;
        let svc = link(&store, "PROJ-1", "svc", 4).await;
        checked(&store, svc.id, PrState::Closed, "feat/x").await;
        link_on(&store, "PROJ-1", 5, Some("feat/x")).await;
        for id in [open.id, merged.id, other.id, svc.id] {
            assert_eq!(store.pr(id).await.unwrap().unwrap().replaced_by, None);
        }
        // Unknown branch (or none): nothing is replaced.
        link_on(&store, "PROJ-1", 6, None).await;
        assert_eq!(store.pr(other.id).await.unwrap().unwrap().replaced_by, None);
    }

    #[tokio::test]
    async fn task_level_insert_refuses_the_pr_kinds() {
        let store = store_with(&["PROJ-1"]).await;
        let key = TaskKey::new("PROJ-1");
        for kind in PR_NOTICE_KINDS {
            assert!(
                store
                    .insert_task_notice(&key, &notice(".", kind, "c"))
                    .await
                    .is_err()
            );
        }
        assert!(
            store
                .insert_task_notice(&key, &notice(".", KIND_BASE_STALE, "c"))
                .await
                .unwrap()
        );
    }
}
