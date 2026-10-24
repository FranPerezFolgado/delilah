# Data model: Task branches start from a fresh default branch

No migration. One new value of an existing column, and new in-memory types.

## `delilah_git::RepoBase` (new, returned by the git calls)

| Field | Type | Meaning |
|---|---|---|
| `repo` | `String` | `.` or the submodule path, as `Workspace::repos` labels it |
| `refname` | `String` | What the branch started from: `origin/main`, `main`, `HEAD`… |
| `sha` | `String` | The commit it started at |
| `fetch` | `FetchOutcome` | `Fetched` \| `NoRemote` \| `Failed(String)` (reason, incl. timeout) \| `Existing` (branch already there, checkout mode: nothing fetched or moved) |

`create_workspace` → `Result<(Workspace, Vec<RepoBase>), GitError>`
`use_checkout` → `Result<(Workspace, Vec<(String, String)>, Vec<RepoBase>), GitError>`
Both take a `Deadline` (start + 30 s, built by the caller).

## `pr_notices` (existing table, migration 0011)

New `kind` value **`base-stale`**:

| Column | Value |
|---|---|
| `repo` | `RepoBase.repo` |
| `kind` | `base-stale` |
| `cause` | `start` |
| `text` | user-language sentence: repo, reason, `refname` @ short sha |
| `detail` | JSON `{"refname": …, "sha": …, "reason": …}` |

One per (task, repo) whose fetch `Failed`. Resolved by dismiss, by archive
(`resolve_all_notices_of_task`) and gone with the task (cascade). The column
comment in 0011 is not edited (shipped migration); the kind list lives in
`archive.rs`-style constants (`KIND_BASE_STALE` in a new `base.rs` or `run.rs`).

## Inbox item kind `base-stale` (new)

`InboxItem { kind: "base-stale", task, title, text, since, notice: Some(id) }`,
not in `BLOCKING`. New optional field `notice` (the notice id) for items that are
notices without a PR.
