# Contracts: Create a task from an idea, with Claude

`crates/delilah-daemon/src/api.rs` (+ `session_api.rs`, `conversation.rs`, `asks.rs`) ↔
`app/src/api.ts`. All changes are additive: existing clients keep working.

## HTTP (delilahd)

### `POST /api/v1/projects/{name}/sessions`: open a project session (changed)

```json
{ "kind": "claude", "conversation": true, "prompt": "opcional: la idea" }
```

- `conversation` is **new** and optional (default `false`, today's behaviour).
- With `true`: `kind` must be `claude` (400 otherwise). `read_only` is ignored: an idea
  conversation is always read-only, through `dontAsk` (research R3, decisions #34), so
  the session's `read_only` field is `false` (enforcement is `dontAsk`, not plan mode)
  and `allow-edits` returns 409 for a conversation. The label is always `"Idea"` (a
  request `label` is ignored). A `prompt`, if given, is recorded as the first user entry
  (`state: "sent"`, `from: "view"`) and typed.
- 201 + `SessionView` with `"conversation": true`. Emits `sessions`.

### `SessionView` (changed)

Gains `"conversation": bool`. It is `false` for every existing session.

### `GET /api/v1/sessions/{id}/conversation` (new)

```json
{
  "entries": [
    { "id": 1, "at": "…", "kind": "user", "text": "…", "state": "delivered", "from": "view" },
    { "id": 2, "at": "…", "kind": "claude", "text": "…" },
    { "id": 3, "at": "…", "kind": "card", "ask": "uuid", "questions": [ … ],
      "proposal": { "title": "…", "brief": "…" }, "outcome": "answered",
      "answer": { "Título": "Crear" }, "task": "LIBRE-7", "error": null }
  ],
  "activity": "Leyendo crates/delilah-daemon/src/api.rs",
  "needs_terminal": false
}
```

- 404 for an unknown session. 409 if it is not a conversation.
- `activity` and `needs_terminal` are `null`/`false` when the session is not live.

### `POST /api/v1/sessions/{id}/messages` (new)

```json
{ "text": "…" }
```

- 400 when the text is empty. 409 if the session is not a conversation, or is not live
  ("Reabrir" first).
- 201 + the entry: `state: "queued"` if the session is working, `"sent"` otherwise (typed
  into the PTY as `inbox::reply` does). Emits `conversations`.

### `DELETE /api/v1/sessions/{id}/messages/{entry}` (new)

204. 409 unless the entry is a `queued` user entry. Emits `conversations`.

### `POST /api/v1/sessions/{id}/stop` (new)

- Writes `ESC` to the PTY, withdraws the session's pending asks, and sets the live
  state to `waiting`. If anything is queued, it is typed at once.
- 204. 409 if not live. Emits `conversations`, `sessions`, `inbox`.
- Valid for any Claude session, but the UI offers it only in the conversation view.

### `POST /api/v1/asks/{id}/answer` (unchanged body, new effect)

When the ask has a `proposal` and the answer to its question is `Crear`:
- The daemon creates the backlog task (data-model "Task") before replying to the hook.
  The answer Claude receives becomes `Crear — creada LIBRE-7 en el backlog`, or
  `Crear — no se pudo crear: <reason>`.
- 200 + `{ "task": "LIBRE-7" }` on success. If creation failed:
  200 + `{ "task": null, "error": "…" }`. The ask is still answered, so Claude can tell
  the user.
- Emits `tasks`, `inbox`, `conversations`.
- A second answer to the same ask: 404, as today (it is resolved).

Any other answer: as today. The response gains `{ "task": null }`.

### `GET /api/v1/inbox` (changed)

`ask.questions[].options[]` may carry `preview` (string, Markdown). `ask` gains
`proposal: { title, brief } | null`.

### `GET /api/v1/tasks/{key}/events` (changed)

It may include `{ "type": "created-from-conversation", "session": "…", "label": "…" }`.

## Events (SSE topics, `events.rs`)

- New topic `conversations`: an idea conversation's entries, activity or
  `needs_terminal` changed. The UI refetches the open conversation.

## Claude launch (`delilah-agents::claude`)

`Launch` gains:
- `plugin_dir: Option<&Path>` → `--plugin-dir <dir>`. Project sessions and idea
  conversations get it; task sessions do not.
- `append_system_prompt: Option<&str>` → `--append-system-prompt <text>`. Idea
  conversations only.
- `permission: Permission` (`Default | Plan | DontAsk`), replacing `read_only: bool`.
  The settings file gains `permissions.allow` / `permissions.deny` for `DontAsk` (R3).

## The plugin (`$DELILAH_HOME/plugins/delilah/`)

```text
.claude-plugin/plugin.json      { "name": "delilah", "description": "…", "version": "<delilah version>" }
skills/task-from-idea/SKILL.md
```

The protocol `SKILL.md` must teach Claude. The daemon depends on it:

1. Ask with AskUserQuestion, never in prose, whenever there are options.
2. Read the code only as far as a mini-analysis needs (minutes, not a spec).
3. The brief has these sections, in Markdown: `## Idea` (the user's words),
   `## Resumen`, `## Zonas afectadas` (paths), `## Preguntas abiertas`.
4. Propose with **one** AskUserQuestion:
   - `header: "Nueva tarea"`;
   - `question: <title>`;
   - options `Crear` (with `preview: <brief>`), `Cambiar algo` and `Descartar`.
5. On `Crear — creada <KEY> …`: tell the user the key. On `Crear — no se pudo crear` or
   a bare `Crear` (the daemon was away): say the task was not created and offer to retry.
6. Never create tasks by any other means, and never use ExitPlanMode.

## CLI

No change: `delilah new` stays as it is.
