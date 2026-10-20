# Data model: Task phase in the task screen header

No stored data changes; no API change. One UI-side derived value.

## HeaderPhase (`app/src/phases.ts`, derived)

| Field | Type | Notes |
|---|---|---|
| `column` | `SpecPhase` | `columnOf(task, spec)`; never `done` (hidden then) |
| `step` | `number` | 1-based position in `COLUMNS` (1–4) |
| `done` | `number?` | only when `column === "implement"` and `spec.total > 0` |
| `total` | `number?` | same condition |

Derived by `headerPhaseOf(task, spec) → HeaderPhase | null`:

| Task status | Spec summary | Result |
|---|---|---|
| `queued` | any | `null` |
| `done` | any | `null` |
| other | none | `null` |
| other | present | `HeaderPhase` |

Inputs (existing): `Task.status`, `Task.phase` (`api.ts`), `SpecSummary { phase, dir, done, total }`.
