# Contracts: daemon HTTP API ↔ client (UI v2)

`crates/delilah-daemon/src/api.rs` (+ the handler modules named below) ↔ `app/src/api.ts`,
and the CLI (`crates/delilah-cli/src/main.rs`) where it uses them.

Rules (brief §6): both sides in this PR; serde shapes and TS types match field by field;
status changes only through `AppState::transition`; emit the named `Topic` after every
change a client must see; each behaviour has a test.

**Compatibility**: everything is additive (new routes, new optional fields, new enum
values in open string unions). Two behaviour changes, no shape change:
- `POST /api/v1/runs` without a workspace choice now works in the checkout (so do
  `delilah run`/`new` without `--checkout`/`--worktree`). A dirty checkout is refused as
  today; the UI shows it as one human error with «Iniciar en un worktree».
- A pending chat-mode permission makes the task `waiting-for-user`.

Old clients: the CLI ignores unknown JSON fields (serde default); it is checked in the
contract review that it does not `deny_unknown_fields` on `Task`, `Session`, `InboxItem`.

## Changed responses

### `GET /api/v1/health`
```json
{ "version": "0.12.0", "exe": "…", "exe_mtime": 0, "home": "/Users/me/.delilah" }
```
TS: `health: () => request<Health>(…)`, `interface Health { version: string; exe?: string | null; exe_mtime?: number | null; home: string }`.

### `GET /api/v1/tasks`, `GET /api/v1/tasks/{key}` — `Task`
`+ "created_at": "2026-10-24T09:12:00Z"` (always present). TS `created_at: string`.

### `GET /api/v1/sessions`, `GET /api/v1/tasks/{key}/sessions` — `Session`
```json
{ "…": "…", "runtime": "pty", "effective_runtime": "stream", "runtime_override": "stream" | null,
  "last_activity_at": "…" | null, "last_message": "…" | null }
```
- `runtime` **keeps its meaning**: the live process's runtime, else the one it would launch
  with today (`session_api::runtime_name`). Its readers are unchanged (T020 lists them).
- `effective_runtime` (NEW): what the next launch of this session uses =
  `runtime_override`, else the task's `chat_mode`. Differs from `runtime` only between a
  switch and the relaunch.
- `runtime_override` (NEW): the stored per-session choice; `null` follows the task.
TS: `runtime: "pty" | "stream"` (now always sent), `effective_runtime: "pty" | "stream"`,
`runtime_override: "pty" | "stream" | null`, `last_activity_at: string | null`,
`last_message: string | null`.

**Words ↔ runtimes (the one mapping, used by every route and the UI):** the request word
`"chat"` = runtime `"stream"` (headless stream-json, chat view only); `"terminal"` = runtime
`"pty"` (TUI; its view may still be chat or terminal on the client, `chooseView`). A PTY
session's Chat | Terminal switch is a client-side view change and calls no route.

### `GET /api/v1/tasks/{key}/prs` — `PullRequest.snapshot`
```json
{ "checks": [{ "name": "ci", "outcome": "pending", "url": null,
               "started_at": "…" | null, "finished_at": "…" | null }],
  "review_comments": [{ "author": "ana", "path": "src/a.rs" | null, "line": 12 | null,
                        "body": "…", "at": "…" }],
  "merge_methods": ["merge", "squash"] }
```
TS: `PrCheck.started_at?: string | null; finished_at?: string | null`; `PrSnapshot.review_comments: ReviewComment[]; merge_methods: MergeMethod[]`; `type MergeMethod = "merge" | "squash" | "rebase"`.

### `GET /api/v1/inbox` — `InboxItem[]`
- `+ "id": "question:LIBRE-5:3f2a…:1729760000000"` on every item.
- `kind` adds `"turn-done"` and `"pr-opened"` (non-blocking). `turn-done` carries
  `session` and `notice`; `pr-opened` carries `pr` and `notice`.
- Dismissed ids are omitted. `inbox_dismissals` is the **single source of truth** for
  dismissals: notice-backed items (`pr`, `notice`) are filtered by joining it too, and
  `POST /pr-notices/{id}/dismiss` writes a dismissal row for that item instead of a separate
  flag, so both routes and restore agree.
- A session with a pending permission ask lists only the `permission` item (no extra
  `waiting` item for it).
- Dismissal rows older than 30 days are purged (at start, at most hourly from `list`).
TS: `InboxItem.id: string`; `InboxKind` += `"turn-done" | "pr-opened"`; `BLOCKING` unchanged.

### `GET /api/v1/sessions/{id}/conversation` — permission cards
Permission card entries (`ask_kind: "permission"`) are **kept** after they end:
```json
{ "kind": "card", "ask_kind": "permission", "outcome": "answered" | "terminal" | "withdrawn" | "pending",
  "tool": "Bash", "summary": "npm test", "rule": "Bash(npm test:*)" | null,
  "decision": "allow_once" | "allow_always" | "deny" | null }
```
TS: `PermissionCardEntry.decision?: PermissionDecision | null`.

### `GET /api/v1/jira/issues`, `GET /api/v1/jira/issues/{key}` — `JiraIssue`
`+ "subtask_count": 20 | null, "subtasks_done": 7 | null`. TS optional numbers.

### `GET /api/v1/tasks/{key}/events` — new `event.type` values
`"reopened"`, `"runtime-switched"` (`session: string | null`, `mode: "chat" | "terminal"`),
`"permission-asked"` (`tool`, `summary`), `"permission-expired"` (`tool`, `summary`, `outcome`). TS: `EventRecord` is already open
(`[field: string]: unknown`); `history.ts` maps them.

### Behaviour: `POST /api/v1/runs`
Resolution of the workspace: `in_checkout` in the request → the backlog task's saved
choice → the project's `settings.workspace` → **checkout** (was: worktree). Shape unchanged.

### Behaviour: task status with permission asks
- Ask inserted while `running` → `waiting-for-user` (`AgentAsked`, history
  `permission-asked`; `Topic::Tasks`, `Topic::Inbox`).
- Only the **last** pending ask of the task (permission or question) answered / withdrawn /
  expired while `waiting-for-user` → `running` (`UserAnswered`).
- A session that ends with an open ask: the ask expires (`permission-expired`,
  `outcome: "withdrawn"`).
- Daemon start: a `waiting-for-user` task whose latest `permission-asked` has no later
  answer/expiry and no live pending ask gets `permission-expired` (`withdrawn`) and stays
  `waiting-for-user`, resumable like any task whose agent died. A `waiting-for-user` from a
  finished turn is untouched.

## New routes

| Method + path | Handler | Body | Response | Errors | Topic |
|---|---|---|---|---|---|
| `POST /api/v1/tasks/{key}/reopen` | `actions::reopen` | — | `Task` | 404 no task; 409 not done, or no workspace (archived) | `Tasks`, `Inbox` |
| `POST /api/v1/tasks/{key}/prs/{id}/merge` | `pr_api::merge::merge` | `{ "method"?: MergeMethod }` | `PullRequest` (refreshed) | 404; 409 draft / not open / checks not passing / `merge_methods` unknown (`[]`) / method not allowed / the `gh` account for the host is unknown (never read) or changed since the PR tab showed it (checked explicitly before running anything); unknown body fields are ignored; 502 gh failure with `AccessProblem` body as other PR routes | `Prs`, `Tasks` |
| `GET /api/v1/tasks/{key}/setup` | `setup::info` | — | `SetupInfo` | 404 no setup ran | — |
| `GET /api/v1/tasks/{key}/setup/log` | `setup::log` | — | `text/plain` (whole log) | 404 no log | — |
| `POST /api/v1/inbox/{id}/dismiss` | `inbox::dismiss` | — (`{id}` percent-encoded by the client: ids contain `:`) | 204 | 404 not listed now; 409 blocking kind, or the item belongs to a project (not a task); a repeat dismiss of a listed item is 204; the notice behind a turn-done / pr-opened aviso stays open, so `/inbox/restore` works for every clearable kind | `Inbox` |
| `POST /api/v1/inbox/clear` | `inbox::clear` | — | `{ "ids": string[] }` (only non-blocking) | — | `Inbox` |
| `POST /api/v1/inbox/restore` | `inbox::restore` | `{ "ids": string[] }` | 204 (unknown ids ignored) | 400 > 500 ids | `Inbox` |
| `GET /api/v1/jira/issues/{key}/subtasks` | `jira::subtasks` | — | `JiraIssue[]` (any assignee/status, ≤ 50) | 404 unknown parent; 409 Jira not connected | — |
| `POST /api/v1/sessions/{id}/runtime` | `runtime_api::set_session_runtime` | `{ "mode": "chat" \| "terminal" }` | 204 | 400 bad mode; 404; 409 not a Claude session, turn running, already in that runtime | `Sessions`, `Tasks` |

**Not a route: «última actividad».** The Bandeja's empty-state line is derived on the client
from `/sessions` (`last_activity_at`, `last_message`, newest first) and, when no session has
one, the newest task event; it refreshes on `Topic::Sessions` and `Topic::Tasks`.

`SetupInfo`:
```json
{ "command": "npm ci", "exit_code": 1, "started_at": "…", "finished_at": "…",
  "duration_ms": 8123, "cancelled": false, "timed_out": false,
  "tail": "…last lines…", "log_bytes": 20480 }
```

TS calls (all in `api.ts`): `reopen(key)`, `mergePr(key, id, method?)`, `setupInfo(key)`,
`setupLogUrl(key)` (a URL for «Abrir el log completo», opened in a new tab/window),
`dismissInbox(id)` (encodes `id` with `encodeURIComponent`), `clearInbox()`, `restoreInbox(ids)`,
`jiraSubtasks(key)`, `setSessionRuntime(id, mode)`.

## Existing routes reused (no change)

- `GET/PUT /api/v1/preferences/{key}` with keys `first-run` and `auto-update`.
- `POST /api/v1/tasks/{key}/runtime` (task-wide; still used by the CLI and as the default
  for new sessions; now also **clears every per-session override** of the task and emits
  `RuntimeSwitched { session: null }`).
- `POST /api/v1/pr-notices/{id}/dismiss` keeps working; it now writes an
  `inbox_dismissals` row for that item (single source of truth), so `/inbox/restore` undoes
  either route.

## CLI (`crates/delilah-cli`)

- `delilah reopen <KEY>` → `POST /tasks/{key}/reopen`; prints the new status.
- `delilah runtime <KEY> <chat|terminal> [--session <ID>]` → with `--session`, the session
  route; otherwise the task route as today.
- `delilah show <KEY>` prints `created`.
- `--checkout` / `--worktree` unchanged; the help text says the default is now the checkout.

## Tauri shell (`app/src-tauri`)

- New command `reveal_path { path }` → `open -R <path>`. The path is **canonicalised first**
  (`..` and symlinks resolved), then must exist and start with the canonical `$HOME`;
  otherwise refused. `platform.ts` `revealInFinder(path)`; no-op outside the desktop.

## UI rules tied to these routes

- «Fusionar» asks first in a `ConfirmDialog` (PR, method, base). With `merge_methods: []` it
  is disabled with «No se pudo saber qué métodos de fusión permite el repositorio»; with
  checks not passing, disabled with the reason.
- The connection observer (`onRequestResult`) is client-only; retries back off to at most
  5 s.

## 409 `reason`

A 409 from the merge route carries `reason` (besides `error`) so the client tells the cases
apart: `account-unknown`, `account-changed`, `not-read`, `not-open`, `draft`, `checks`,
`merge-methods-unknown`, `method-not-allowed`. When `gh` itself cannot be used the reason is
the `AccessProblem` kind (e.g. `signed-out`).
