# Implementation Plan: Task phase in the task screen header

**Branch**: `feature/FEAT-11` | **Date**: 2026-10-20 | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `specs/003-task-phase-header/spec.md`

## Summary

The task screen header gets a phase badge (Especificación / Plan / Implementación /
Revisión) before the status pill, so the phase is in sight while the agent terminal
fills the screen. It is frontend-only. `App` already holds the specs map that the board
uses, and it now passes the task's `SpecSummary` to `TaskScreen`. A new pure helper in
`phases.ts` decides what the header shows, reusing `columnOf`, so the header, the board
and the spec panel always agree. A small `PhaseBadge` component renders it, styled like
the spec panel's current step, and adds "· done/total" while in Implementación. See
[research.md](research.md).

## Technical Context

**Language/Version**: TypeScript + React (Tauri app, `app/`)

**Primary Dependencies**: React, Vite; vitest + Testing Library

**Storage**: none (data already in the app: `Task.phase`, `SpecSummary`)

**Testing**: `cd app && npx tsc --noEmit && npx vitest run` (helper in `phases.test.ts`, component in `PhaseBadge.test.tsx`)

**Target Platform**: macOS (Tauri app); the web UI in a preview for trying it

**Project Type**: desktop-app (web UI only for this feature)

**Performance Goals**: no extra requests; the header updates on the same SSE topics (`tasks`, `specs`) as the board

**Constraints**: same phase as board and spec panel; readable at the window's minimum width (1024 px, `tauri.conf.json`)

**Scale/Scope**: one header, one helper, one component

## Constitution Check

`.specify/memory/constitution.md` is still the unfilled template; the project's
ground rules (`CLAUDE.md`) serve as gates instead:

| Gate | Status |
|---|---|
| Nothing from the work repos; fixtures only | ✅ tests use in-memory task/spec objects |
| Agent hooks/settings outside the repo, clean env | ✅ not touched |
| Try changes only in a preview, never 7420 / pkill | ✅ quickstart uses `scripts/preview.sh` |
| No remote operations without confirming the account | ✅ none |
| Contracts (`api.rs` ↔ `api.ts`) | ✅ unchanged; no API change |

Post-design re-check: no change.

## Project Structure

### Documentation (this feature)

```text
specs/003-task-phase-header/
├── spec.md
├── plan.md
├── research.md
├── data-model.md
├── quickstart.md
├── contracts/ui.md
└── checklists/{requirements,ux}.md
```

### Source Code

```text
app/src/phases.ts                     # headerPhaseOf(task, spec) → { column, step, progress? } | null
app/src/phases.test.ts                # headerPhaseOf cases
app/src/components/PhaseBadge.tsx     # new: the badge (step number, label, "· done/total"), title/aria-label
app/src/components/PhaseBadge.test.tsx
app/src/components/TaskScreen.tsx     # Props.spec?: SpecSummary; badge before the status pill in the header
app/src/App.tsx                       # passes spec={specs[owner]} to TaskScreen
app/src/styles.css                    # .phase-pill (the stepper's "now" look), no shrink for header pills
app/src/i18n/en.ts                    # "Fase: {phase}" → "Phase: {phase}"
README.md, docs/status.md             # user-facing note and status
```

**Structure Decision**: existing layout. The only new file is a small component, kept
outside `TaskScreen`, which is lazy-loaded and heavy (editor, terminal), so it can be
tested on its own.

## Design

### 1. What to show: `headerPhaseOf` (`phases.ts`)

```ts
export interface HeaderPhase { column: SpecPhase; step: number; done?: number; total?: number }
export function headerPhaseOf(task: Task, spec?: SpecSummary): HeaderPhase | null
```

- `null` when `task.status.type` is `queued` or `done`, or when `spec` is undefined.
  These are the cases where the board has no phase column, or where the spec panel
  shows nothing.
- Otherwise `column = columnOf(task, spec)`, which is the same rule as the board and the
  spec panel. `step` is its 1-based index in `COLUMNS`.
- `done`/`total` only when `column === "implement"` and `spec.total > 0`.
- Failed and paused-by-limit tasks keep their phase, so you can see where they stopped.

### 2. The badge (`PhaseBadge.tsx`)

- `<span className="pill phase-pill" title=… aria-label=…>` holding the step number in
  a `dot-step`, then `phaseLabel(column)`, then ` · {done}/{total}` when present.
- `title` / `aria-label` = `t("Fase: {phase}", …)` plus the count when it is shown.
- It is display-only, with no click handler. The board and the spec panel's stepper
  still change the phase.

### 3. Header (`TaskScreen.tsx`)

- New prop `spec?: SpecSummary`. Inside the `task && status` branch, compute
  `headerPhaseOf(task, spec)`.
- Order: owner, title, **phase badge**, status pill, file-tree toggle. The left margin
  set to `auto` moves to the first of the two pills that is present (the badge when
  shown, else the status pill).
- The project header (no task) is left unchanged.

### 4. Styles (`styles.css`)

- `.phase-pill`: the look of `.stepper li.now`, with `border-color: var(--acc)`,
  `background: var(--accbg)` and `color: var(--tx)`. It is set apart from the status
  pill, which uses coloured text, an icon and a neutral border.
- `.view-head .pill { flex-shrink: 0; }` so the title (`.trunc`, `min-width: 0`) is the
  first to shrink.

### 5. Data flow

`App` → `specs[owner]` (already refreshed on the `specs` SSE topic, with polling as the
fallback) and `task` (`tasks` topic) → `TaskScreen` → `headerPhaseOf`. This is the same
data the board renders, so a change reaches both at the same time (SC-003).

## UX checklist resolution (`checklists/ux.md`)

| Item | Resolution |
|---|---|
| CHK001 position | Before the status pill: `[phase] [status] [tree]` (§3) |
| CHK002 icon/text | Step number + label, no icon (§2) |
| CHK003 count format | `Implementación · 4/12` (§1, §2) |
| CHK004 display-only | Yes (§2) |
| CHK005 distinct | Stepper "now" look vs. coloured status text + icon (§4) |
| CHK006 rule | `columnOf(task, spec)` (§1) |
| CHK007 no spec info | `specs[owner]` undefined: no workspace, archived, or not loaded yet (§1) |
| CHK008 vs. spec panel | Both hide without a spec; both use `columnOf` |
| CHK009 count vs. board | Same `spec.done`/`spec.total`; header only in implement (spec FR-005) |
| CHK010 names | `phaseLabel` / `COLUMNS`, already translated |
| CHK011 failed/paused | Phase shown (§1) |
| CHK012 loading | Nothing shown until the specs map has the task; no placeholder |
| CHK013 narrow width | Title truncates; pills don't shrink (§4) |
| CHK014 min width | 1024 px (window minimum) |
| CHK015 SC-003 | Same SSE topics as the board (§5) |
| CHK016 a11y | `aria-label` / `title` "Fase: …" (§2) |

## Testing

- `phases.test.ts`, `headerPhaseOf`:
  - queued → null; done → null; no spec → null;
  - running task with `phase: "plan"` → plan, step 2;
  - no recorded phase → the spec's phase; spec with no folder → `specify`;
  - implement with total 12 / done 4 → count; implement with total 0 → no count; plan with a total → no count;
  - failed and paused tasks → phase shown.
- `PhaseBadge.test.tsx`:
  - renders the label and step;
  - renders "· 4/12" only when it gets a count;
  - `aria-label` matches the label.
- `tsc` covers the new `TaskScreen` prop and the `App` wiring.

## Complexity Tracking

None.
