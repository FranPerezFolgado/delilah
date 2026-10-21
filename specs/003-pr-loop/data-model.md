# Data model: PR loop

Migration `crates/delilah-daemon/migrations/0011_pull_requests.sql`.

## `pull_requests`

One PR linked to a task, per repo of its workspace.

| Column | Type | Notes |
|---|---|---|
| `id` | INTEGER PK | |
| `task_key` | TEXT NOT NULL | FK `tasks(key)` ON DELETE CASCADE |
| `repo` | TEXT NOT NULL | the repo inside the workspace: `.` or a submodule path |
| `host` | TEXT NOT NULL | e.g. `github.com` |
| `owner_name` | TEXT NOT NULL | `owner/name` |
| `number` | INTEGER NOT NULL | |
| `url` | TEXT NOT NULL | |
| `linked_by` | TEXT NOT NULL | `detected` · `opened` · `pasted` |
| `snapshot` | TEXT | JSON `Snapshot`, NULL until the first check |
| `ci_failed_sha` | TEXT | head already notified as failed |
| `review_seen` | INTEGER NOT NULL DEFAULT 0 | watermark |
| `issue_comment_seen` | INTEGER NOT NULL DEFAULT 0 | watermark |
| `review_comment_seen` | INTEGER NOT NULL DEFAULT 0 | watermark |
| `checked_at` | TEXT | last successful check (RFC 3339) |
| `error` | TEXT | last check's problem (`AccessProblem` name + message), NULL when fine |
| `created_at` | TEXT NOT NULL | |

- `UNIQUE (task_key, repo)`: one PR per repo of a task; linking another replaces it.
- A merged or closed PR stays for reference after the task is archived.

## `pr_unlinked`

PRs the user unlinked, never detected again for that task (FR-025).

| Column | Type |
|---|---|
| `task_key` | TEXT NOT NULL, FK CASCADE |
| `url` | TEXT NOT NULL |

PK `(task_key, url)`.

## `pr_notices`

What the Bandeja shows for PRs.

| Column | Type | Notes |
|---|---|---|
| `id` | INTEGER PK | |
| `task_key` | TEXT NOT NULL | FK CASCADE |
| `repo` | TEXT NOT NULL | |
| `kind` | TEXT NOT NULL | `ci-failed` · `changes-requested` · `comments` · `archive-blocked` · `archive-partial` |
| `cause` | TEXT NOT NULL | head sha, review id, highest comment id, or `merge` |
| `text` | TEXT NOT NULL | shown in the Bandeja (already in the current language) |
| `detail` | TEXT | JSON: failing checks or the comments, used by "Pedir al agente" |
| `created_at` | TEXT NOT NULL | |
| `resolved_at` | TEXT | NULL while it shows |

- `UNIQUE (task_key, repo, kind, cause)`: inserting the same notice again is a no-op.
- Resolved by: a later green head (`ci-failed`), merge/close of the PR, archive or
  delete of the task (cascade on delete), forward or dismiss.

## Settings

- `pr_access:<project>` = access problem already notified for that project (cleared on
  success).

## Rust types

```rust
// delilah-daemon/src/prs.rs (pure parts unit-tested)
pub struct Snapshot {
    pub state: PrState,            // Open | Merged | Closed
    pub draft: bool,
    pub title: String,
    pub head: String,              // sha
    pub base: String,
    pub checks: Vec<Check>,        // name, outcome: Pending|Success|Failure|Neutral, url
    pub review_decision: Option<ReviewDecision>, // Approved | ChangesRequested | ReviewRequired
    pub reviews: Vec<Review>,      // id, author, state, body
    pub comments: Vec<Comment>,    // id, kind: Issue|Review, author, path, line, body
}
pub struct Marks { ci_failed_sha: Option<String>, review: i64, issue_comment: i64, review_comment: i64 }
pub enum NoticeDraft { CiFailed { sha, checks }, ChangesRequested { review }, Comments { up_to, comments } }
pub enum Info { Approved, Merged, Closed }
pub enum AccessProblem { Missing, SignedOut, NoAccess, UnsupportedRemote, Network, RateLimited { reset: Option<DateTime<Utc>> } }

pub fn ci_outcome(checks: &[Check]) -> Rollup;            // Pending | Success | Failure | None
pub fn diff(prev: Option<&Snapshot>, marks: &Marks, next: &Snapshot, me: &str)
    -> (Vec<NoticeDraft>, Marks, Vec<Info>);
pub fn pick(prs: &[ListedPr]) -> Option<&ListedPr>;     // open > merged > closed, newest first
```

## State flow of a linked PR

```
(none) ──detect / open / paste──▶ open|draft ──merge──▶ merged ─(all merged)─▶ task done + archive
                                      │                                  └─ blocked → archive-blocked notice
                                      └──close──▶ closed (notify only; task unchanged)
```

Task effects: linking moves the phase to `review` if below (FR-027); all merged →
`MarkDone` (if not terminal) → archive (R7).
