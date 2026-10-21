# UI contract: Task phase in the task screen header

No HTTP contract changes (`api.rs` ↔ `api.ts` untouched).

## `headerPhaseOf(task: Task, spec?: SpecSummary): HeaderPhase | null` (`phases.ts`)

See [data-model.md](../data-model.md). Pure; same phase as `columnOf`.

## `<PhaseBadge phase={HeaderPhase} />` (`components/PhaseBadge.tsx`)

- Renders `<span class="pill phase-pill">` with the step number, `phaseLabel(column)`
  and, when `total` is set, ` · {done}/{total}`.
- `title` and `aria-label`: `t("Fase: {phase}", { phase })`, plus the count when shown.
- No click behaviour.

## `TaskScreen` props

- New optional `spec?: SpecSummary` (from `App`: `specs[owner]`).
- Header order with a task: owner · title · PhaseBadge (when not null) · status pill · tree toggle.

## i18n

| es (code) | en |
|---|---|
| `Fase: {phase}` | `Phase: {phase}` |
