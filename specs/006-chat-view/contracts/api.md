# Contracts: Chat view and chat mode

`crates/delilah-daemon/src/api.rs` (+ `conversation.rs`, `session_api.rs`, `terminal.rs`,
new `live.rs`) ↔ `app/src/api.ts`, and the CLI (`crates/delilah-cli`).

**Not additive** (only the app reads these; the CLI does not, and this is checked in the
contract review):
- the entry `id` type changes from number to string, in responses and in the
  `DELETE …/messages/{entry}` path parameter;
- `GET …/conversation` returns 200 instead of 404/409 for non-idea Claude sessions.

**Additive** (existing clients tolerate them):
- new fields on the conversation response, `Task` and `Session`;
- new routes;
- new inbox item kind.

## Changed

### `GET /api/v1/sessions/{id}/conversation?cursor=<c>`

- Works for **every Claude session** (it used to be 404/409 unless `conversation = 1`).
  Shell sessions: 409 `"a shell session has no conversation"`.
- Response:
  ```json
  {
    "entries": [Entry],
    "cursor": "12345",
    "complete": true,
    "transcript": "present" | "pending" | "missing",
    "activity": "…" | null,
    "needs_terminal": false,
    "runtime": "pty" | "stream"
  }
  ```
  - Without `cursor`: everything. With `cursor`: transcript entries after it, plus all
    stored entries (the client replaces stored entries and appends the rest).
  - `complete: false` means more transcript is available right away (a page cap of 500
    entries); the client asks again.

### `POST /api/v1/sessions/{id}/messages`, `DELETE …/messages/{entry}`, `POST …/stop`

- Allowed for every live Claude session, not only idea conversations.
- In chat mode, `stop` interrupts the stream turn (R7-e), and pending permission asks
  are released as denied.
- `POST …/messages`: typed now when Claude is idle, queued when it works or a card is
  pending. Deciding and typing is one step per session (two sends never interleave).
  When the message is typed and earlier messages are still queued (e.g. after `stop`),
  they are typed first, in order, in the same paste, and all marked sent. At most 20
  queued messages and 64 000 queued characters per session: beyond that, `409`.
  A closed or shell session gives `409`.
- `stop` keeps the queue (queued messages are not sent); it goes out ahead of the next
  message sent, or at the next turn's end.
- `DELETE …/messages/{entry}`: `404` for a non-numeric id or a transcript (`t:…`) id;
  `409` when the message is no longer queued.
- Cards carry `tool_use_id` (only when it is a safe id: at most 128 characters of
  `[A-Za-z0-9_-]`). A card replaces the AskUserQuestion tool entry with the same
  `tool_use_id`, on full and incremental responses alike, and carries that call's `at`.
- Hide rules once a transcript exists: stored user entries that are `delivered` are
  hidden (`queued` and `sent` stay visible), and for idea conversations stored `claude`
  entries are hidden too.

### `GET /api/v1/sessions/{id}/terminal`, `GET /api/v1/tasks/{key}/terminal` (attach)

- Chat-mode session: `409 {"error": "the session runs in chat mode: no terminal. Switch the task to the terminal with `delilah runtime <task> terminal` or from the app"}`.

### `POST /api/v1/tasks` (create) and `PATCH /api/v1/tasks/{key}`

- New optional fields:
  - `view: "chat" | "terminal"` on both;
  - `chat_mode: bool` on create only (switching goes through `/runtime`).
- `Task` JSON gains `view`, `chat_mode`, and `allowed_tools: string[]`.

### Session JSON (`/sessions`, `/tasks/{key}/sessions`)

- Gains `runtime: "pty" | "stream"`. It comes from the live session, or from the task's
  flag when the session is not live.

### Inbox items

- A new item kind, `permission`, for chat-mode permission asks. It carries the tool, a
  one-line summary of the input, and the suggestion. Answered with the endpoint below.

## New

### `GET /api/v1/sessions/{id}/conversation/tools/{tool_use_id}`

`{ "input": {...}, "result": "…full text…", "is_error": false, "truncated": false }`.
404 if not found. `truncated` is true when the input or the result was cut to 1 MB.

A tool entry's `path` (in `GET …/conversation`) is relative to the session's workspace
when the file is inside it (so the view can open it in the editor); it stays absolute
when the file is outside, and the view then offers no "Abrir en el editor".

### `GET /api/v1/sessions/{id}/live` (WebSocket, chat mode)

Server → client JSON messages: `{"message": "<msg id>", "text": "<delta>"}`, then
`{"message": "<msg id>", "done": true}`. Nothing is replayed. On a PTY session the
socket closes at once with code 4409 and the reason `"not in chat mode"`.

### `POST /api/v1/sessions/{id}/permissions/{ask}`

Body: `{"decision": "allow_once" | "allow_always" | "deny", "reason"?: string}`.
- 204 on success.
- 404 if the ask is gone.
- `allow_always` appends the suggestion (or the tool name) to `tasks.allowed_tools`.

### `POST /api/v1/tasks/{key}/runtime`

Body: `{"mode": "chat" | "terminal"}`.
- 204, after the live sessions are relaunched with `--resume` in the new runtime.
- 409 `"wait for the turn to end or press Detener"` while a session works or has a
  pending ask.
- 409 when chat mode is not supported by the installed Claude.

## Topics (`/api/v1/events`)

No new topic. `conversations` is now also emitted when any live session's transcript
grows. `tasks` is emitted on view, chat-mode and allowed-tools changes. `inbox` is
emitted on permission asks.

## CLI

- `delilah new` and `delilah run` take `--chat` (chat mode on, for a new task).
- `delilah runtime <task> chat|terminal`.
- `delilah attach` on a chat-mode task prints the API's 409 message and exits 1.
