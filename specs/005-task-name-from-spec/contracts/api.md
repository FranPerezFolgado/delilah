# Contracts: Task name and branch from the first analysis

Changes to `crates/delilah-daemon/src/api.rs` ↔ `app/src/api.ts`. All additive.

## `Task` (every endpoint that returns one)

```json
{ "key": "LIBRE-12", "title": "Task name and branch from the first analysis",
  "title_source": "spec", "previous_title": "arreglar lo de las ramas",
  "branch_error": null, "…": "…" }
```

- `title_source`: `"initial" | "spec" | "manual"`.
- `previous_title`: `string | null`.
- `branch_error`: `string | null`.

## `PATCH /api/v1/tasks/{key}`

- `title` is now accepted for any task that isn't archived (was: backlog only). The
  other backlog fields (`project`, `workflow`, `notes`, `in_checkout`) keep the
  backlog-only rule: 409 as today for a started task.
- A title change sets `title_source = "manual"` and `previous_title = null`.
- Empty title → 400 (unchanged).

## `POST /api/v1/tasks/{key}/branch` (new)

Retries creating/applying the task's branch.
- 200 with the `Task` (its `branch_error` null on success, set on failure).
- 404 unknown task; 409 the task isn't a free task, or already has its branch in every repo.

## `POST /hooks/claude/guard?task=&session=` (new, internal)

Body: the PreToolUse hook JSON. Response: empty (allow), or
`{"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"deny","permissionDecisionReason":"<why>"}}`.
Only registered in sessions launched for a branchless task.

## `POST /api/v1/tasks/{key}/prs`

- New refusal: 422 `"la tarea aún no tiene rama"` / `"the task has no branch yet"` for a branchless task.

## Inbox

- New item kind `"branch"`: `{ kind: "branch", task, title, text: <branch_error> }`; its action is the retry above.

## CLI

- No new command; `delilah ls` shows the new title. `delilah show` (if it prints the task) shows `previous_title` when set.
