# Contracts: Jira import (HTTP API and CLI)

All additive; `app/src/api.ts` gets matching types and functions. Errors use the existing
`ApiError` body `{ error: <code>, message }` (the code is `bad-key`, `project-required`,
`not-a-subtask`, `stale` or a refusal code).

## `GET /api/v1/jira/issues/{key}/import`

Preview (R1). `key` must be an issue key (400 `bad-key` otherwise). An optional `?project=` previews attach and
conflicts for the project the dialog chose.

200:

```json
{
  "linked": true,
  "ticket": { "key": "PROJ-10", "title": "Checkout", "status": "In Progress", "is_subtask": false },
  "parent": { "action": "create" },
  "subtasks": [
    {
      "key": "PROJ-11", "title": "API", "status": "To Do", "status_category": "new",
      "assignee": null, "mine": false,
      "task": null, "action": "create",
      "skipped": false, "selected": true
    }
  ],
  "gone": [ { "key": "PROJ-9", "title": "Old piece" } ]
}
```

- `parent.action`: `create` | `convert` | `existing` | `refused` (with `code`, `message`;
  codes from spec 012: `nesting`, `open-pr`, `running-session`, `archived`, …).
- `subtasks[].action`: `create` | `attach` | `linked` | `conflict` (with `reason`; the other rows
  omit `reason`).
- `subtasks[].task`: `{ key, kind, status, parent_task, archived }` or `null`.
- Jira not connected → 200 `{ "linked": false, "reason": "not-connected" }`; ticket unknown in
  Jira → 200 `{ "linked": false, "reason": "not-found" }`; Jira error/timeout → 200
  `{ "linked": false, "reason": "unreachable" }`. The parent task screen stays quiet on all
  three (a note only for `not-connected`); the dialog shows `unreachable` with retry. A key that
  is not an issue key never reaches Jira (`not-found`).

## `POST /api/v1/jira/issues/{key}/import`

```json
{ "project": "signal", "add": ["PROJ-11"], "skip": ["PROJ-12"], "unskip": [] }
```

- `project` is required when `parent.action` is `create` (400 `project-required`), and ignored
  otherwise (the parent task's project wins).
- Unknown keys in `add`/`skip`/`unskip` (not sub-tasks of the ticket in Jira) → 400
  `not-a-subtask`, nothing written. `skip`/`unskip` need an existing or created parent task.
- Parent refused → 409 with the refusal `code`, nothing written.
- Jira not linked, ticket not found or Jira unreachable → 409 (with `reason`), nothing written.
- A key in both `add` and `skip` is skipped, not added.

200:

```json
{
  "parent_task": "PROJ-10",
  "created": ["PROJ-11"], "attached": [],
  "left_out": [ { "key": "PROJ-13", "reason": "started" } ],
  "skipped": ["PROJ-12"], "unskipped": []
}
```

Emits `Topic::Tasks` once after commit when a task was created, converted or attached.

An empty `add` is valid. The write re-checks inside its transaction what may have changed
since the preview: a convert that is no longer allowed → 409, nothing written; an attach or
create whose key is no longer free or eligible → `left_out` (`taken`, `started`, …).

## `GET /api/v1/tasks/{key}/subtasks` (unchanged)

Jira status per subtask (US4) comes from the preview's `subtasks[].status` matched by key in
the UI, not from this route.

## CLI: `delilah jira import`

```
delilah jira import <KEY> [--project <P>] [--defaults | --add K1,K2]
                          [--skip K1,K2] [--unskip K1,K2]
```

- No `--defaults/--add/--skip/--unskip`: prints the preview (parent action, one row per
  sub-task with key, status, assignee, action, `*` for selected) and writes nothing.
- `--defaults`: adds the preselected ones; `--add`: exactly those.
- Prints the result (`created`, `attached`, `left out: KEY (reason)`); exit code 1 on a refusal.
