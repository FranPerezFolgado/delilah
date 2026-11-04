# Data model: Track every PR a task opens

## Migration `0027_pr_per_url.sql`

SQLite cannot drop a table constraint, so both tables are rebuilt (`CREATE … _new`,
`INSERT … SELECT` with explicit ids, `DROP`, `ALTER … RENAME`, then re-create any
indexes), keeping every column (0011, 0012 `error_kind`, `autofix`, …), value and `id`.
(`PRAGMA foreign_keys` is a no-op inside sqlx's migration transaction; the copy order —
new table first, drop old, rename — needs no FK toggling.) A migration test loads data
shaped like 0011..0026 and checks ids, `autofix` JSON, `error_kind` and the `pr_url`
backfill survive (SC-004).

### `pull_requests`

| Column | Change |
|---|---|
| all existing columns (0011, 0012 `error_kind`, later `autofix`, …) | kept as they are |
| `replaced_by INTEGER NULL` | new: id of the PR that replaced this closed one (FR-009); plain integer, no FK; unlinking the newer PR clears it in `unlink_pr` |
| `UNIQUE (task_key, repo)` | **dropped** |
| `UNIQUE (task_key, url)` | **new** |

`repo` meaning: `.` (plain repo, or a parent's own repo), a submodule path, or
`host/owner/name` for a repo outside the workspace. `linked_by`: `detected`, `opened`,
`pasted`, **`agent`** (new).

Existing data: at most one row per `(task_key, repo)` today and URLs are canonical, so the
new unique key holds for every existing row.

### `pr_notices`

| Column | Change |
|---|---|
| `pr_url TEXT NOT NULL DEFAULT ''` | new: the PR the notice is about; `''` for task-level notices |
| `UNIQUE (task_key, repo, kind, cause)` | replaced by `UNIQUE (task_key, repo, pr_url, kind, cause)` |

Backfill: `pr_url` = the `url` of the `pull_requests` row with the same `(task_key, repo)`
for kinds `ci-failed`, `changes-requested`, `comments`, `pr-opened`; `''` for the rest
(`archive-blocked`, `archive-partial`, base-stale). Notices of those PR kinds whose PR row
is gone are marked resolved by the migration (they could never be resolved otherwise).

### `pr_unlinked`

Unchanged (already per URL).

## Snapshot (JSON in `pull_requests.snapshot`)

| Field | Change |
|---|---|
| `head_branch: string \| null` | new, from `headRefName`; `#[serde(default)]` so old snapshots read as `null` |

## Derived rules

- **Counted PR**: `replaced_by IS NULL`.
- **All merged** (archive): at least one counted PR, every counted PR's snapshot `Merged`.
- **Replacement**: on linking PR N with repo R and head branch B, every counted PR of the
  task with repo R, head branch B and snapshot `Closed` gets `replaced_by = N`, and its
  open notices are resolved.
- **Repos with a PR (Open PR, detection skip)**: repos having a counted PR, state `Open` or
  `Merged`, whose head branch is the task's branch.
- **Archive heads**: counted merged PRs with a workspace label (`pr_repos`) and head branch
  = task branch or `null` (old detected rows) → `label → head sha`; two in one label → the
  highest id wins.
- **Subtask rollup** (`integration.rs`): reads counted PRs only.

## State of a task (unchanged states, new conditions)

`Review` → `Done/archived` only when all merged, the primary session is not `Working`,
and a forced re-detection in `pr_repos` found nothing unmerged.
