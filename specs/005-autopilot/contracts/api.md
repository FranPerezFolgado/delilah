# Contract: Autopilot (daemon API ↔ `app/src/api.ts`, CLI)

All changes are additive: old clients ignore the new fields.

## `Task`

```ts
autopilot: null | {
  state: "running" | "paused" | "done";
  stage?: "preview" | "opening_pr";
  reason?: string;
  since: string;          // RFC 3339
  nudged?: Step;
};
```

## Creating and starting

- `RunRequest` gains `autopilot?: boolean`. `true` forces `saving_mode: true`; `autopilot: true` with `saving_mode: false` is `400`.
- `NewBacklogTask` and `BacklogChange` (`PATCH /api/v1/tasks/{key}`) gain `autopilot?: boolean`, same rule.

## `POST /api/v1/tasks/{key}/autopilot`

Body `{ "action": "on" | "off" | "resume" }` → `200 Task`.

- `on`: sets saving mode too; on a running task, a pending planning → implementation offer is confirmed at once. `409 reason "not-spec-workflow"` for tasks that cannot have saving mode.
- `off`: clears autopilot (saving mode stays).
- `resume`: only when `state = paused` (else `409 reason "not-paused"`); continues from the first unfinished step.

## `POST /api/v1/tasks/{key}/saving-mode` (existing)

`{ "on": false }` now also clears `autopilot`.

## Preview

- `GET /api/v1/tasks/{key}/preview` → `200 PreviewView | null`
- `POST /api/v1/tasks/{key}/preview` → starts it; `201 PreviewView` (`state: "starting"`). `409 reason "no-preview-command"` if the project has none, `"no-workspace"` before the task has one, `"running"` if one is already starting/running.
- `DELETE /api/v1/tasks/{key}/preview` → `204`.

```ts
interface PreviewView { state: "starting" | "running" | "failed" | "stopped"; url?: string; error?: string; started_at: string }
```

## Projects

- `Project` gains `preview_command: string | null`.
- `PATCH /api/v1/projects/{name}` accepts `preview_command?: string | null` (empty string = none).

## Inbox

- `InboxKind` gains `"autopilot"`: one item per paused autopilot task, `text` = the reason, actions in the UI call `POST /tasks/{key}/autopilot` with `resume` or `off`. Not in `BLOCKING` (the task's own blocking item, e.g. a question, is listed separately).

## Asks

- `POST /api/v1/asks/{id}/answer` body gains `launch?: "autopilot"`, only meaningful when answering `Crear` on a task proposal: the created task starts at once with autopilot on. Response unchanged (`{ task, error? }`; `error` set when the start fails, the task stays in the backlog).

## History

New event types in `GET /api/v1/tasks/{key}/events`: `autopilot-paused {reason}`, `autopilot-resumed`, `autopilot-answered {question, answer}`, `autopilot-nudged {step}`, `autopilot-finished {prs}`.

## CLI

`delilah new` and `delilah run` gain `--autopilot` / `--no-autopilot` (conflicts with `--no-saving-mode`). `delilah autopilot <KEY> on|off|resume`.
