# Data model: Chat view and chat mode

## Migrations (new, never edit a shipped one)

- `0016_task_view.sql` (Phase 3):
  ```sql
  ALTER TABLE tasks ADD COLUMN view TEXT NOT NULL DEFAULT 'chat'; -- 'chat' | 'terminal'
  ```
- `0017_chat_mode.sql` (Phase 6, only if the spike keeps chat mode):
  ```sql
  ALTER TABLE tasks ADD COLUMN chat_mode INTEGER NOT NULL DEFAULT 0;
  ALTER TABLE tasks ADD COLUMN allowed_tools TEXT; -- JSON array of permission rules; NULL = []
  ```

Both are additive, and existing rows keep today's behaviour (chat view, PTY runtime, no
extra rules).

## `Task` (delilah-core / store.rs)

| Field | Type | Notes |
|---|---|---|
| `view` | `TaskView` (`Chat` \| `Terminal`), default `Chat` | Display only. Set at creation and by `PATCH /tasks/{key}` |
| `chat_mode` | `bool`, default `false` | Runtime of the task's Claude sessions. Set at creation and by `POST /tasks/{key}/runtime` |
| `allowed_tools` | `Vec<String>` | Rules granted with "Permitir siempre en esta tarea". Merged into `permissions.allow` of the task's session settings |

## Conversation entry (API shape; not all of it is stored)

Common: `id: string`, `at: RFC 3339`, `kind`.

| kind | Source | Fields |
|---|---|---|
| `user` | transcript, or stored (queued / no transcript) | `text`, `state` (`queued`\|`sent`\|`delivered`), `from` (`view`\|`terminal`) |
| `claude` | transcript, or stored (no transcript) | `text` (Markdown) |
| `tool` | transcript | `tool` (name), `tool_use_id`, `summary` (one line), `input` (object, large strings truncated), `result` (string\|null while pending), `is_error`, `truncated`, `diff` (rows, Edit/MultiEdit/Write only), `path` (when the tool targets a file) |
| `card` | stored (asks) | as today, plus `tool_use_id` when known; `ask_kind`: `question` (today) \| `permission` (chat mode: `tool`, `input`, `suggestion`) |
| `marker` | transcript or stored | `marker`: `compacted` \| `stopped` |

IDs:
- transcript entries: `t:<line uuid>:<block index>`;
- stored entries: their row id as a string.

The id type changes from number to string. The app and the daemon ship together; the
CLI does not read entries (checked in the contract review).

**Diff rows** (the same shape ChangesView renders):
`{ kind: 'add'|'del'|'ctx'|'hunk', old: number|null, new: number|null, text: string }`.

## Transcript cursor

An opaque string: the byte offset of the first unread complete line, base-10. The
response gives `cursor` for the next call. Stored entries are always returned in full;
there are few of them.

## Live session (sessions.rs, in memory)

- `pty: Arc<PtySession>` → `runtime: Runtime` (`Pty` | `Stream`).
- `transcript: Mutex<Option<PathBuf>>`: from the latest hook's `transcript_path`.
- No task per session: one daemon-wide watcher loop (`transcript_watch.rs`) covers every live session.

## Permission ask (permissions.rs, in memory; chat mode)

`PermissionAsk { id, task, session, request_id, tool, input, suggestion: Option<String>, since }`
lives in its own registry, not in `asks.rs`: there is no blocking hook. The answer goes
through `StreamSession::reply_permission`. It is released (denied with "sesión detenida")
on stop, kill or switch. The Inbox lists both registries.

## `Live::is_conversation` → `Live::is_idea`

After Phase 4, the composer and the entries API work for every Claude session. The flag
only marks idea conversations: the Nueva tarea card, proposals, and read-only. It is
renamed to say so. The column `sessions.conversation` keeps its name (shipped).

## State transitions

- `tasks.chat_mode` changes only between turns (`POST /runtime`, 409 otherwise), and at
  creation.
- A live session's runtime never changes in place: a switch kills it and relaunches it
  with `--resume`.
