# Tasks: Task phase in the task screen header

**Input**: Design documents from `specs/003-task-phase-header/`
**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/ui.md, quickstart.md

**Tests**: test-first (vitest). Write each test task before its implementation and see it fail.

**Checks**: `cd app && npx tsc --noEmit && npx vitest related --run <files>`; at the end `npx vitest run`.

## Format: `[ID] [P?] [Story] Description`

## Phase 1: Setup

None: no new dependencies; the existing vitest + Testing Library setup is used as is.

---

## Phase 2: Foundational (blocks both stories)

**Purpose**: the pure rule for what the header shows, and its text.

- [ ] T001 Write failing tests for `headerPhaseOf(task, spec)` in `app/src/phases.test.ts`, reusing the file's `task()` factory:
  - `queued` → null; `done` → null; no spec (undefined) → null.
  - running task with `phase: "plan"` and a spec → `{ column: "plan", step: 2 }`.
  - `phase: null` with spec `phase: "implement"` → `implement`, step 3; spec with `dir: null` and no recorded phase → `specify`, step 1.
  - `failed` and `paused-by-limit` tasks with a spec → phase shown.
- [ ] T002 Implement `export interface HeaderPhase { column: SpecPhase; step: number; done?: number; total?: number }` and `export function headerPhaseOf(task: Task, spec?: SpecSummary): HeaderPhase | null` in `app/src/phases.ts`. It returns null for `queued`, for `done` and when there is no spec. Otherwise `column = columnOf(task, spec)` (never `done` here), and `step` = its index in `COLUMNS` + 1. Leave `done`/`total` unset (US2). T001 then passes.
- [ ] T003 [P] Add `"Fase: {phase}": "Phase: {phase}"` to `app/src/i18n/en.ts`, next to the existing phase strings (e.g. after `"Mover a {phase}"`).

**Checkpoint**: `npx vitest related --run src/phases.ts` passes.

---

## Phase 3: User Story 1: See the task's phase while working with the agent (P1) 🎯 MVP

**Goal**: the task screen header shows the phase before the status pill. It matches the board and the spec panel, and it updates live.

**Independent Test**: in a preview, open a running task whose phase is "plan". The header reads "2 Plan" before the status pill with the terminal in front. Moving the phase with the spec panel's stepper changes it without a reload.

- [ ] T004 [P] [US1] Write failing tests in `app/src/components/PhaseBadge.test.tsx` (pattern: `Sidebar.test.tsx`):
  - `<PhaseBadge phase={{ column: "plan", step: 2 }} />` renders the step "2" and the label "Plan" in an element with classes `pill phase-pill`.
  - Its `aria-label` and `title` are "Fase: Plan", which is the Spanish source, as the other tests use.
  - It renders no button / is not clickable.
- [ ] T005 [US1] Create `app/src/components/PhaseBadge.tsx`. It exports `PhaseBadge({ phase }: { phase: HeaderPhase })` and renders `<span className="pill phase-pill" title={label} aria-label={label}><span className="dot-step">{phase.step}</span>{phaseLabel(phase.column)}</span>`, where `label = t("Fase: {phase}", { phase: phaseLabel(phase.column) })`. T004 then passes.
- [ ] T006 [P] [US1] Add the styles in `app/src/styles.css`, after the `.pill` rule (~line 69):
  - `.phase-pill { border-color: var(--acc); background: var(--accbg); color: var(--tx); }`
  - a compact `.phase-pill .dot-step` that fits the 20 px pill (look at the `.dot-step` rule (~line 207) for the existing look and size it down);
  - `.view-head .pill { flex-shrink: 0; }` so the title (`.trunc`) shrinks first.
- [ ] T007 [US1] In `app/src/components/TaskScreen.tsx`:
  - add `spec?: SpecSummary` to `Props` and to the destructured props;
  - in the header's `task && status` branch (~line 439), compute `const phase = headerPhaseOf(task, spec)`;
  - render `{phase && <PhaseBadge phase={phase} />}` before the status pill;
  - give `style={{ marginLeft: "auto" }}` to the badge when it is shown, and to the status pill only when it is not. Wrap the badge in nothing extra; pass the style through a `style` prop on `PhaseBadge`, or a `className` such as `push-right`, whichever is simpler.
  - The project header branch stays unchanged.
- [ ] T008 [US1] In `app/src/App.tsx` (~line 322), pass `spec={specs[owner]}` to `<TaskScreen>`.

**Checkpoint**: `npx tsc --noEmit` and `npx vitest related --run src/components/PhaseBadge.tsx src/phases.ts` pass. US1 is usable on its own.

---

## Phase 4: User Story 2: See implementation progress in the header (P2)

**Goal**: in Implementación with a task list, the badge reads "Implementación · 4/12".

**Independent Test**: a task in "implement" whose spec has 4 of 12 done shows "· 4/12". Another phase, or `total: 0`, shows no count.

- [ ] T009 [P] [US2] Extend the tests in `app/src/phases.test.ts`:
  - implement with spec `done: 4, total: 12` → `done: 4, total: 12`;
  - implement with `total: 0` → no `done`/`total`;
  - plan with `total: 12` → no `done`/`total`.
- [ ] T010 [P] [US2] Extend `app/src/components/PhaseBadge.test.tsx`:
  - `{ column: "implement", step: 3, done: 4, total: 12 }` renders "· 4/12", and the `aria-label` includes "4/12";
  - without `total`, no "·" is rendered.
- [ ] T011 [US2] In `headerPhaseOf` (`app/src/phases.ts`), set `done`/`total` from the spec when `column === "implement"` and `spec.total > 0`. T009 then passes.
- [ ] T012 [US2] In `PhaseBadge` (`app/src/components/PhaseBadge.tsx`), append `<span className="muted"> · {done}/{total}</span>` when `total` is set, and add ` · {done}/{total}` to the `title`/`aria-label`. T010 then passes.

**Checkpoint**: all of `npx vitest run` passes.

---

## Phase 5: Polish & cross-cutting

- [ ] T013 [P] `README.md`: in the "Desktop app" section, where it describes the task screen,, add one line saying the header shows the task's phase (and the progress while implementing) next to its status.
- [ ] T014 [P] `docs/status.md`: add the phase in the task header to what is built (keep the file's style).
- [ ] T015 Run `cd app && npx tsc --noEmit && npx vitest run`, then follow `specs/003-task-phase-header/quickstart.md` "By hand" in a preview (`scripts/preview.sh`, never 7420). Check the five scenarios: phase, live change, progress, hidden cases, 1024 px width.

---

## Dependencies & execution order

- Phase 2: T001 → T002; T003 in parallel.
- US1: depends on T002 (and T003 for the text). T004 ∥ T006 → T005 → T007 → T008.
- US2: depends on US1's `PhaseBadge` (T005) and T002. T009 ∥ T010 → T011, T012 (different files, so they can run in parallel).
- Polish: after US1 (T013/T014 can be written once US2 is decided); T015 last.

## Parallel examples

- Phase 2: T001 + T003.
- US1: T004 (badge test) + T006 (CSS), then T005; T007 and T008 touch different files but T007 needs T005.
- US2: T009 + T010, then T011 + T012.
- Polish: T013 + T014.

## Implementation strategy

- **MVP**: Phase 2 + US1. The header shows the phase, which is the whole request.
- **Then**: US2 (count), then polish and the hand check in a preview.
- Commit after each phase.
