# Contract: Saving mode (daemon API ↔ `app/src/api.ts`)

All changes are additive: old clients ignore the new fields.

## Types

```ts
type Step = "plan" | "implement" | "review";
type HandoffStage = "offered" | "saving" | "starting" | "failed" | "skipped";
interface Handoff { to: Step; stage: HandoffStage; error?: string }

interface Task { /* … */ saving_mode: boolean; handoff: Handoff | null }
interface Session { /* … */ step: Step | null }
type InboxKind = /* … */ | "handoff";   // ordered after "review", before "suggest-review"
```

## `POST /api/v1/runs`

`RunRequest` gains `saving_mode?: boolean`. Absent: the `saving-mode` preference. Kept for backlog tasks like `in_checkout`.

## `POST /api/v1/tasks/{key}/saving-mode`

Body `{ "on": boolean }` → `200 Task`. Off clears a pending handoff (sessions stay). `404` unknown task.

## `POST /api/v1/tasks/{key}/handoff`

Body `{ "action": "confirm" | "decline" | "retry" }` → `200 Task`.

| Action | Allowed when | Effect |
|---|---|---|
| `confirm` | `stage = offered` | → `saving` (save-notes on) or `starting` |
| `decline` | `stage = offered` | handoff cleared, `saving_mode = false` |
| `retry` | `stage = failed` | → `starting` |

`409` when the stage does not allow the action (e.g. a double click after it already started); `404` unknown task.

## Inbox item `handoff`

`text` says what is waiting: the offer ("Las tareas están listas. ¿Implementar en una sesión nueva?"), "Guardando las notas de la sesión…", the start ("Pasando a la revisión en una sesión nueva…" for the automatic review, "Empezando la sesión nueva…" otherwise; transient: the item goes away once the new session is running), or the failure. `session` is the outgoing primary. The UI shows the buttons by `task.handoff.stage`.

## CLI

`delilah new` / `delilah run` gain `--saving-mode` / `--no-saving-mode`.
