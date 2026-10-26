# Contract changes: daemon HTTP API ↔ `app/src/api.ts` ↔ CLI

All changes are additive except the stored `Task.workflow` value (`feature` → `project`),
which requests still accept as an alias.

## `GET /api/v1/projects`, `GET /api/v1/projects/{name}`

`settings` gains `workflow` (absent ⇒ single/builtin):

```json
{ "settings": { "workspace": null, "branch": null, "shared": [], "setup": null,
  "submodules": {"always": [], "never": []},
  "workflow": { "kind": "phased", "plan": "/feature {task}",
                "implement": "/orchestrate {spec_dir}", "review": "/review de la rama …" } } }
```

`Project.workflow` (top-level string): unchanged, deprecated, not used to choose the start.

## `PUT /api/v1/projects/{name}/settings`

Body: the whole `ProjectSettings`, now with `workflow`. Errors: 422 `{error, fields}` where
`fields` is a `FieldError[]` for the codes in data-model.md (`workflow.blank`,
`workflow.unknown_placeholder`, `workflow.placeholder_not_allowed`,
`workflow.unbalanced_brace`), e.g.

```json
{ "error": "invalid settings",
  "fields": [ { "field": "workflow.implement", "code": "workflow.unknown_placeholder",
                "message": "unknown placeholder {foo}" } ] }
```

## `GET /api/v1/projects/{name}/skills` (new)

200:

```json
[ { "name": "fix", "description": "Fix a bug from a ticket", "source": "project" },
  { "name": "feature", "description": "Spec-driven …", "source": "user" },
  { "name": "frontend-design:frontend-design", "description": "…", "source": "plugin",
    "plugin": "frontend-design" } ]
```

404 unknown project. Never 500 because of an unreadable skill file (skipped).

## `POST /api/v1/runs` (start) and `POST /api/v1/tasks` (create / backlog)

| Field | Before | After |
|---|---|---|
| `workflow` | free string, default project's (`feature`) | `project` \| `prompt` \| `none`; `feature` accepted = `project`; anything else 400. **`POST /tasks` without `workflow` stores `project`** (follows the project's workflow) |
| `prompt` | runs: first message, sent as typed | runs: first message (implies `prompt`) and now **a template**: `{title}` `{ticket}` `{task}` expand, an unknown placeholder or a lone brace is a 422, literal braces are doubled (`{{` `}}`); **tasks: new**, stored as `start_message`. `workflow: "prompt"` needs a non-blank `prompt` |
| `saving_mode`, `autopilot` | — | ignored/forced off when the resolved start is not phased; `autopilot: true` there → 409 `not-spec-workflow` (unchanged code); turning saving mode on later (`PATCH`, `POST …/saving-mode`) on a task that is not phased → 409 `not-phased` |

## `Task` (all task responses)

- `workflow`: `project` | `prompt` | `none`.
- `start_message`: string | null (**new**).

## `PATCH /api/v1/tasks/{key}` (backlog edit)

Accepts `workflow` and `start_message`. `workflow` is validated strictly (`project` | `prompt` |
`none`; `feature` is accepted as `project`; anything else 400); `start_message` is validated like
the settings templates (422 `{error, fields}`), and `prompt` needs a non-blank message. Refuses `autopilot` when the result would not be phased (409, unchanged).

## CLI

- `delilah run` / `delilah new`: `--prompt <text>` unchanged; new `--no-prompt` (start `none`);
  `--workflow` accepts only `project|prompt|none|feature` (the CLI rejects anything else).
- `delilah project show <name>` prints the workflow (kind and commands).
- Editing the workflow from the CLI is out of scope for v1 (the settings screen does it);
  `delilah project unset <name> workflow` goes back to the built-in prompt.
- `delilah project add --workflow`: accepted, ignored, prints a deprecation warning.
