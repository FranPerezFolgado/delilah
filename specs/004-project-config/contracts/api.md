# API contract: Per-project configuration

Daemon `crates/delilah-daemon/src/api.rs` ↔ client `app/src/api.ts`.

## `Project` (every response that returns projects)

```jsonc
{
  "name": "plataforma", "path": "/…", "kind": "parent", "workflow": "feature",
  "settings": {                       // new; always present, defaults filled in
    "workspace": "checkout",
    "branch": "feature/{ticket}",
    "shared": [{ "path": ".env", "mode": "copy" }, { "path": "node_modules", "mode": null }],
    "setup": null,
    "submodules": { "always": [], "never": [] }
  }
}
```

Additive: older clients ignore `settings`.

## `PUT /api/v1/projects/{name}/settings`

Body: a whole `ProjectSettings` (missing fields = defaults). Replaces the stored
settings.

- `200` → the updated `Project`; emits the `projects` topic.
- `404` → no such project.
- `422` → `{ "error": "<summary>", "fields": [{ "field": "shared[0].path", "code": "shared.path_absolute", "message": "…" }] }`;
  nothing saved. `code` is stable and machine-readable; the daemon localises `message`
  from `code`. Codes: `branch.empty`, `branch.missing_ticket`,
  `branch.unknown_placeholder`, `shared.path_empty`, `shared.path_absolute`,
  `shared.path_parent`, `shared.path_duplicate`, `submodules.not_parent`,
  `submodules.conflict`.

## `POST /api/v1/runs` (unchanged shape, new defaults)

When `in_checkout` is absent and the task has no choice saved, it comes from
`settings.workspace`. When `branch` is absent, from `settings.branch`. `submodules` is
merged: `+ always`, and a request naming a `never` submodule gets `422` naming it.
With a `setup` and a worktree, the response is `201` with the task in
`preparing-workspace`; the agent launches later.

## `GET /api/v1/projects/{name}/submodules` and `POST …/suggest-submodules`

`never` submodules are left out of both responses.

## Task status

`TaskStatus` gains `{ "type": "setup-failed", "reason": "…" }`.

## `POST /api/v1/tasks/{key}/setup/retry` · `POST /api/v1/tasks/{key}/setup/skip`

Only from `setup-failed` (`409` otherwise). `202`; the task goes to
`preparing-workspace`. Retry runs the setup again; skip launches the agent.

## `POST /api/v1/tasks/{key}/setup/cancel`

Only while a setup runs (`409` otherwise). Kills it; the task goes to `setup-failed`
with reason "cancelled".
