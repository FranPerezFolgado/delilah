# Data model: Task backlog and deleting tasks

## Task (`delilah-core::Task`, table `tasks`)

| Field | Change | Notes |
|---|---|---|
| `key` | — | Jira key or `LIBRE-n`. Can't change after creation. |
| `title` | editable while `Queued` | |
| `project` | required for new backlog tasks; editable while `Queued` | Existing rows may have `NULL` (removed project): shown under "Sin proyecto". |
| `notes` | **new**, `Option<String>`, `#[serde(default)]` | Free text / Markdown. Migration `0009_task_notes.sql`: `ALTER TABLE tasks ADD COLUMN notes TEXT;` |
| `in_checkout` | **new**, `Option<bool>` | Where it will work once started: checkout (`true`) / worktree (`false`); `NULL` not chosen (the UI shows checkout). Migration `0010_task_in_checkout.sql`. A start request that leaves it out uses it. |
| `workflow` | defaults to the project's | |
| `status` | — | `Queued` = backlog. |

## State transitions (`next_status`)

```text
            ┌──────────── StartAborted (new) ───────────┐
            ▼                                           │
 [Queued = backlog] ── Start ──▶ PreparingWorkspace ── WorkspaceReady ──▶ Running …
            │                          │
            │                          └── Fail ──▶ Failed      (direct start only)
            │
            └── (delete) ──▶ ∅          any state but PreparingWorkspace ──(delete)──▶ ∅
```

- `StartAborted { reason }`: only from `PreparingWorkspace`; recorded in `task_events`.
- Nothing goes back to `Queued` except `StartAborted`.
- Delete isn't an event: the row and its history are gone.

## Cascade on delete (already in the schema, `foreign_keys(true)`)

| Table | FK | Effect |
|---|---|---|
| `task_events` | `task_key → tasks ON DELETE CASCADE` | history removed |
| `workspaces` | `task_key → tasks ON DELETE CASCADE` | removed (released before) |
| `sessions` | `task_key → tasks ON DELETE CASCADE` | removed, with `attention` (inbox) |

In memory (`AppState`): `sessions` (live PTYs), `asks`, review hints. These are cleared by
the delete handler.

On disk: `~/.delilah/tasks/<key>/` (ticket.md, notes.md, claude-*.json, *.log),
removed best effort.

## Settings

| Key | Value | Use |
|---|---|---|
| `free_key_last` | JSON number | last `LIBRE-n` handed out (R7) |
