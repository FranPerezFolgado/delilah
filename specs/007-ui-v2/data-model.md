# Data model: UI v2

Everything here is additive. Existing rows keep their meaning except one behaviour change:
a project with `settings.workspace = null` and a task with no saved `in_checkout` now
work in the **checkout** (R9; no data changes).

## Migrations (new; never edit a shipped one — last shipped is `0019_chat_mode.sql`)

### `0020_inbox_dismissals.sql` (T012)

```sql
-- Avisos the user removed from the Bandeja ("Quitar", "Limpiar avisos"); restorable.
-- `id` is the computed item id (kind:task:source:since_ms), so a new occurrence of the
-- same cause has a new id and shows again.
CREATE TABLE inbox_dismissals (
    id           TEXT PRIMARY KEY,
    task_key     TEXT NOT NULL REFERENCES tasks (key) ON DELETE CASCADE,
    dismissed_at TEXT NOT NULL
);
CREATE INDEX inbox_dismissals_by_task ON inbox_dismissals (task_key);
```

### `0021_session_activity.sql` (T020)

```sql
-- Last turn activity of a session, for the project view's conversation rows.
ALTER TABLE sessions ADD COLUMN last_activity_at TEXT;   -- RFC 3339, NULL until the next turn
ALTER TABLE sessions ADD COLUMN last_message TEXT;       -- clipped to 200 chars
-- A runtime of the session's own; NULL: the task's chat_mode decides.
ALTER TABLE sessions ADD COLUMN runtime TEXT;            -- 'pty' | 'stream' | NULL
```

The `task_key` of a project session is `@<project>` (`PROJECT_OWNER`), which is not in
`tasks`; dismissals only ever reference real task keys (project sessions raise no avisos).

## On-disk files (no migration)

- `<task_dir>/setup.json` (T018) — a file-based exception to the database, like
  `launch.json` (decision 55): written at the end of every setup run (finish, cancel,
  timeout), overwritten on retry, removed with the task dir when the task is deleted; never
  read for state, only for `GET …/setup`:
  ```json
  { "command": "npm ci", "exit_code": 1, "started_at": "…", "finished_at": "…",
    "cancelled": false, "timed_out": false }
  ```
  The log stays where it is today (the path `output_tail` reads).

## `Task` (delilah-core `task.rs`, `store.rs`)

| Field | Type | Notes |
|---|---|---|
| `created_at` | `DateTime<Utc>` | NEW in the API. Already in `tasks.created_at` (migration 0001); `store.rs` reads it |

**Status machine** (`next_status`), new row:

| From | Event | To |
|---|---|---|
| Done | `Reopen` | WaitingForUser |

Unchanged rows now reached from new places: `Running --AgentAsked--> WaitingForUser` and
`WaitingForUser --UserAnswered--> Running` are also sent by `permissions.rs` (R8), with
these rules: only the **last** pending ask (permission or question) of the task sends
`UserAnswered`; a session ending with an open ask expires it (`PermissionExpired`,
`withdrawn`); at daemon start, a `WaitingForUser` task whose latest `permission-asked` has no
later answer/expiry and no live ask gets `PermissionExpired` (`withdrawn`) and stays waiting
(resumable).

**New events** (history only, no status change; `next_status` returns the same status):

| Event | `name()` | Payload |
|---|---|---|
| `Reopen` | `reopened` | — |
| `RuntimeSwitched` | `runtime-switched` | `session: Option<String>`, `mode: "chat" \| "terminal"` |
| `PermissionAsked` | `permission-asked` | `tool: String`, `summary: String` (lets the startup reconcile find waits caused by a permission) |
| `PermissionExpired` | `permission-expired` | `tool: String`, `summary: String`, `outcome: "terminal" \| "withdrawn"` |

## `Session` (`store_sessions.rs`)

| Field | Type | Notes |
|---|---|---|
| `last_activity_at` | `Option<DateTime<Utc>>` | NEW; set on `Stop`, `UserPromptSubmit`, and a sent message |
| `last_message` | `Option<String>` | NEW; Claude's last message on `Stop`, the prompt on submit |
| `runtime` | `"pty" \| "stream"` | **Unchanged meaning**: the live process's runtime, else what it would launch with (`session_api::runtime_name`); now always serialised |
| `effective_runtime` | `"pty" \| "stream"` | NEW: `runtime_override`, else the task's `chat_mode` — what the next launch uses |
| `runtime_override` | `Option<"pty" \| "stream">` | NEW; the stored override (NULL = follows the task). `POST /tasks/{key}/runtime` clears it on every session of the task |

## Inbox item (`inbox.rs`)

| Field | Type | Notes |
|---|---|---|
| `id` | `String` | NEW, computed: `<kind>:<task>:<session id \| notice id \| ->:<since unix ms>` |
| `kind` | `ItemKind` | NEW kinds `turn-done`, `pr-opened` (non-blocking) |

`ItemKind::is_blocking()` = `permission, question, error, waiting, pr-failed, pr-review,
pr-blocked` — the same list as `BLOCKING` in `api.ts`; a shared JSON fixture
(`crates/delilah-daemon/tests/fixtures/blocking_kinds.json`, read by a Rust test and by
`app/src/vocab.test.ts`) keeps them equal.

`pr_notices` rows gain two `kind` values (text column, no schema change): `turn-done`
(`repo` = `'.'`, text = last message) and `pr-opened` (the PR's repo and URL). The table is
reused on purpose although these are not PR notices; a rename to a neutral `notices` is
recorded as pending (decisions.md, T101).

**Dismissals**: `inbox_dismissals` is the single source of truth, also for notice-backed
items (the notice list joins it; `POST /pr-notices/{id}/dismiss` writes a row). Rows older
than 30 days are purged at start and at most hourly.

## PR snapshot (`pr_types.rs`, stored as JSON in `pull_requests.snapshot`)

| Type | Field | Notes |
|---|---|---|
| `PrCheck` | `started_at?`, `finished_at?` | NEW, optional (old snapshots lack them) |
| `PrSnapshot` | `review_comments: ReviewComment[]` | NEW, `#[serde(default)]` so stored snapshots still parse |
| `PrSnapshot` | `merge_methods: ("merge"\|"squash"\|"rebase")[]` | NEW, default `[]` (unknown) |
| `ReviewComment` | `author: String, path: Option<String>, line: Option<u32>, body: String, at: DateTime<Utc>` | NEW; newest 50 |

## Setup info (API only)

`SetupInfo { command: String, exit_code: Option<i32>, started_at, finished_at:
Option<…>, duration_ms: Option<u64>, cancelled: bool, timed_out: bool, tail: String,
log_bytes: u64 }`.

## Jira issue (`delilah-integrations::jira`)

| Field | Type | Notes |
|---|---|---|
| `subtask_count` | `Option<u32>` | NEW; `None` for a subtask itself |
| `subtasks_done` | `Option<u32>` | NEW; children with status category `done` |

## Health

`Health { version, exe, exe_mtime, home: String }` — `home` NEW (absolute `DELILAH_HOME`).

## Preferences (existing table via `/preferences/{key}`)

| Key | Value | Default |
|---|---|---|
| `first-run` | `{ "jira_skipped": bool, "dismissed": bool }` | absent = `{false,false}` |
| `auto-update` | `bool` | absent = `true` |

## Client-only state

- Connection (`connection.ts`): `{ state: "up"|"down", lastOk: number, downSince?: number }`; retries every 2 s, at most 5 s apart.
- «Última actividad» (Bandeja empty state): derived from `/sessions` `last_activity_at`/`last_message`, else the newest task event; no route.
- Sidebar rail forced on/off: `localStorage["delilah.rail"]` (per window, try/catch).
- Board filters (project, status): App state, set by the statusbar counts.
