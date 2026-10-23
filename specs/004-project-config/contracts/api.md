# API contract: Per-project configuration

Daemon `crates/delilah-daemon/src/api.rs` ↔ client `app/src/api.ts`.

## `Project` (every response that returns projects)

```jsonc
{
  "name": "plataforma", "path": "/…", "kind": "parent", "workflow": "feature",
  "settings": {                       // new; always present, defaults filled in
    "workspace": "checkout",
    "branch": null,                  // optional template (needs {ticket}); null: Delilah proposes the name
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
  `submodules.conflict`, `submodules.never` (a task request names a `never` submodule),
  `submodules.stale_rule` (a rule names a submodule the parent no longer has),
  `submodules.unknown` (a rule names a submodule path the parent does
  not have), `settings.invalid` (malformed body, unknown key or wrong type; `field`
  names the key when known, else is empty). A body with an unknown key or a wrong type is also `422`, with `fields`
  naming the key when known.

## `PATCH /api/v1/projects/{name}` (kind change)

Changing `kind` from `parent` to `repo` clears `settings.submodules` (both lists), since
a plain repo has no submodules.

## `POST /api/v1/runs` (unchanged shape, new defaults)

When `in_checkout` is absent and the task has no choice saved, it comes from
`settings.workspace`. When `branch` is absent (or blank), from `settings.branch` if the project has a template, else from Delilah's proposal (task branch names); a typed branch is kept as typed with a template and normalised by `custom_for` without one. `POST /api/v1/projects/{name}/branch-name` returns the template-expanded branch when the project has one. `submodules` is
merged: `+ always` (request order first, no duplicates; an empty selection is valid when
`always` is not), and a request naming a `never` submodule gets `422` naming it, with the
`{error, fields}` shape: field `submodules[i]`, code `submodules.never`. A rule naming a
submodule the parent no longer has gets `422` with field `settings.submodules.always[i]`
(or `never[i]`), code `submodules.stale_rule`, and a message pointing at the project's settings.
With a `setup` and a worktree, the response is `201` with the task in
`preparing-workspace`; the agent launches later.

## `GET /api/v1/projects/{name}/submodules` and `POST …/suggest-submodules`

`never` submodules are left out of both responses. `GET …/submodules?all=true` returns every
submodule, `never` ones included; the project settings screen uses it to edit the rules.

## Task status

`TaskStatus` gains `{ "type": "setup-failed", "reason": "…" }`.

## `POST /api/v1/tasks/{key}/setup/retry` · `POST /api/v1/tasks/{key}/setup/skip`

Only from `setup-failed` (`409` otherwise). `202`; the task goes to
`preparing-workspace`. Retry runs the setup again; skip launches the agent.

## `POST /api/v1/tasks/{key}/setup/cancel`

Only while a setup runs (`409` otherwise). Kills it; the task goes to `setup-failed`
with reason "cancelled".
