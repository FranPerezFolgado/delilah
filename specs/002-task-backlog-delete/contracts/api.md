# Contracts: Task backlog and deleting tasks

## HTTP (delilahd)

### `POST /api/v1/tasks`: create a backlog task (changed)

```json
{ "key": "PROJ-12", "title": "…", "project": "delilah", "notes": "…", "free": false, "workflow": null }
```

- `free: true` → `key` may be empty; Delilah assigns `LIBRE-n` (never reused).
- `project` must exist (404 otherwise); `workflow` defaults to the project's. The app and
  `delilah new` always send it; the API still takes a bare task (no project) for driving the
  state machine by hand and in tests.
- 201 + `Task` (`status: {"type":"queued"}`, `notes`). 409 if the key exists.
- Emits `tasks`.

### `PATCH /api/v1/tasks/{key}`: edit a backlog task (new)

```json
{ "title": "…", "notes": "…", "project": "…", "in_checkout": true }
```

All fields are optional. 200 + `Task`; 409 if the task isn't `queued`; 404 for an unknown project.

### `POST /api/v1/runs`: start (changed)

Same body as today. If `key` names an existing **queued** task:
- that task is started; `title` defaults to the task's (the body's wins if given);
  its notes are passed to the agent (`notes.md`).
- 201 + `Task`. 409 if it is already starting or started.
- If the workspace fails: 422, and the task goes back to `queued` (`start-aborted` event).

An existing key in any other state: 409, as today.

### `DELETE /api/v1/tasks/{key}[?force=true]`: delete (new)

- 204 when deleted. Emits `tasks`, `sessions`, `inbox`, `specs`.
- A task in the project's checkout never counts as losing work (its branch and folder stay);
  only a worktree is checked.
- 404 unknown key.
- 409 `{"error": "PROJ-12 is preparing its workspace; try again in a few seconds"}` while `preparing-workspace`.
- 409 `{"error": "…uncommitted/unpushed…", "unpushed": true}` when the worktree has work that would be lost
  and `force` is not set. Nothing was stopped.
- 409 when the workspace can't be released (git error); the task is kept.

### `Task` JSON (changed)

`+ "notes": string | null`, `+ "in_checkout": boolean | null`

`POST /tasks` also takes `in_checkout`; `POST /runs` with no `in_checkout` uses the task's
(else a worktree). CLI: `delilah new … --checkout`.

### Events

`{"type":"start-aborted","reason":"workspace: …"}` in `/tasks/{key}/events`.

## CLI (`delilah`)

| Command | Change |
|---|---|
| `delilah new [KEY] --title T --project P [--notes N] [--free]` | `--project` required; `--notes`, `--free` new. Prints the key. |
| `delilah run KEY …` | With an existing backlog `KEY`, starts it (`--title` optional then). |
| `delilah rm KEY [--force]` (alias `delete`) | New. Exit 0 when deleted; on unpushed work, exit 1 with "…; use --force to delete it anyway". |
| `delilah ls` | Shows `backlog` for `queued` tasks. |

## UI strings (es → en, `app/src/i18n/en.ts`)

"Guardar en backlog" → "Save to backlog", "Backlog" → "Backlog", "Notas" → "Notes",
"Empezar" → "Start", "Eliminar" → "Delete", "Eliminar igualmente (se pierde lo no subido)"
→ "Delete anyway (unpushed work is lost)", "¿Eliminar {key}?" → "Delete {key}?",
"Se parará el agente." → "The agent will be stopped.", "Sin proyecto" → "No project".
