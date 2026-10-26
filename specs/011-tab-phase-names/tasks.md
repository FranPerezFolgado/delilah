# Tasks: Session tabs named after their phase

**Input**: Design documents from `specs/011-tab-phase-names/`
**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/ui.md, quickstart.md

**Tests**: Test-first (project convention in `CLAUDE.md` and the orchestrate workflow). Each
story writes its failing vitest cases before the code.

**Checks**: `cd app && npx tsc --noEmit && npx vitest related --run <files>`. No Rust changes.

## Format: `[ID] [P?] [Story] Description`

## Phase 1: Setup

No setup: no new dependency, module or config. `app/` is already set up.

---

## Phase 2: Foundational (blocks every story)

**Purpose**: The single naming rule that the tab bar and the Sessions list both use (FR-001
to FR-005, research R2/R3).

- [ ] T001 Write failing unit tests for `sessionName` in `app/src/chain.test.ts` (new `describe("sessionName")`, with `setLanguage("es")` in `beforeEach`, restored after). Build sessions with the same full `Session` literal shape as `TaskScreen.test.tsx`'s `sess()`. Cases:
  - step `plan` / `implement` / `review` → «Spec y plan» / «Implementación» / «Revisión», whether `primary` is true or false and whether `live` is true or false;
  - after `setLanguage("en")`, the same → «Spec and plan» / «Implement» / «Review»;
  - primary `claude` with `step: null` → «Claude principal» (en «Main Claude»);
  - primary of kind `shell` with `step: null` → its own `label` (pins the plan's choice of the `kind === "claude"` rule);
  - non-primary with `step: null` → its own `label`;
  - the stored `label` is never used when `step` is set (use a label like `"Otra cosa"`).
- [ ] T002 Implement and export `sessionName(s: Session): string` in `app/src/chain.ts`, next to `stepLabel`: `s.step ? stepLabel(s.step) : s.primary && s.kind === "claude" ? t("Claude principal") : s.label`. Use no cast (a `Step` already fits `ChainStepName`), and add a one-line doc comment. Make T001 pass.

**Checkpoint**: `npx vitest related --run src/chain.ts` green; `npx tsc --noEmit` clean.

---

## Phase 3: User Story 1 - The current phase session's tab says which phase it is (Priority: P1) 🎯 MVP

**Goal**: The primary session's tab reads its step's name; a primary without a step still reads «Claude principal».

**Independent Test**: Render `TaskScreen` with a primary session whose `step` is `implement`. The tab list contains «Implementación» and no «Claude principal».

- [ ] T003 [US1] Add failing tests in `app/src/components/TaskScreen.test.tsx` under `describe("TaskScreen tabs")`:
  - `taskScreen([sess("a", { primary: true, step: "implement", label: "Implementación" })])` → `labels()` equals `["Tarea", "Oficina", "Implementación", "PR"]`;
  - the same with `step: "review"` → «Revisión», and with `step: "plan"` → «Spec y plan»;
  - the phase tab's `title` attribute is the stored label (use a stored label that differs from the step name, for example `label: "Implementación: Mi tarea"`, to prove it is kept as the tooltip, FR-006);
  - the primary phase tab has no close button (`queryByRole("button", { name: "Cerrar Implementación" })` is null).
  The existing test "lists Tarea, the primary as Claude principal and the other sessions" stays unchanged as the FR-002 regression guard.
- [ ] T004 [US1] In `app/src/components/TaskScreen.tsx` (tab items, around line 639), replace `label: s.primary ? t("Claude principal") : s.label` with `label: sessionName(s)` (import from `../chain`). Keep `title: s.label` and `closable: !s.primary`. Make T003 pass, and keep every existing `TaskScreen` test green.

**Checkpoint**: US1 works on its own. In a preview, a saving-mode task's current phase tab reads its phase.

---

## Phase 4: User Story 2 - Earlier phase sessions keep their phase name (Priority: P2)

**Goal**: Former phase sessions (no longer primary, live or stopped) read their step's name; extra Claudes and terminals keep their label.

**Independent Test**: Render `TaskScreen` with a non-primary stopped `implement` session and a primary `review` session. The tabs read «Implementación», «Revisión», in that order.

- [ ] T005 [US2] Add tests in `app/src/components/TaskScreen.test.tsx`:
  - `[sess("old", { step: "implement", live: false, closed: false }), sess("new", { primary: true, step: "review" })]` → the session tabs read «Implementación», «Revisión» in that order;
  - the former phase tab is closable, and its close button is named «Cerrar Implementación»;
  - an extra Claude `sess("b", { label: "Review" })` with `step: null` keeps «Review».
  These should pass with T004 alone. If one fails, fix `TaskScreen.tsx` (the session list order comes from the `sessions` prop and must stay as it is).

**Checkpoint**: US1 and US2 both pass.

---

## Phase 5: User Story 3 - The task's Sessions list uses the same names (Priority: P2)

**Goal**: The «Sesiones» list on the task tab names each session as the tab bar does (FR-004, SC-003).

**Independent Test**: Render `TaskView` with a primary `implement` session. Its `.tv-sname` reads «Implementación».

- [ ] T006 [P] [US3] Add failing tests in `app/src/components/TaskView.test.tsx`, using the file's `show(...)` helper and `session(...)` factory:
  - a primary `claude` session with `step: "implement"` → its `.tv-sname` text is «Implementación»;
  - a non-primary stopped session with `step: "plan"` → «Spec y plan»;
  - a primary `claude` with `step: null` → «Claude principal» (regression guard).
- [ ] T007 [US3] In `app/src/components/TaskView.tsx` (around line 269), replace `x.primary && x.kind === "claude" ? t("Claude principal") : x.label` with `sessionName(x)` (import from `../chain`). Make T006 pass and keep the existing `TaskView` tests green.

**Checkpoint**: For any session list, `TaskView` row names equal `TaskScreen` tab labels.

---

## Phase 6: Polish & Cross-Cutting Concerns

- [ ] T008 [P] Update `docs/usage.md`, in the task screen's "Sessions" bullet (around line 477). Say that in saving mode each phase's session is named after its phase («Spec y plan», «Implementación», «Revisión»), that this also holds once a session is no longer the current one, and that outside saving mode the primary reads «Claude principal».
- [ ] T009 [P] Add a line to `docs/status.md` (the task screen / saving mode part) saying that session tabs and the Sessions list name phase sessions after their step (LIBRE-24).
- [ ] T010 [P] Check the e2e selectors in `app/e2e/chat.spec.ts` (lines 89, 170) and `app/e2e/screens.spec.ts` (line 218), which look up the tab «Claude principal». Confirm that the tasks those tests create are not in saving mode, so the selector still matches. If one creates a saving-mode task, change its selector to the step's name. Don't run the e2e suite against the installed app.
- [ ] T011 Run the full checks: `cd app && npx tsc --noEmit && npx vitest run`. All green.
- [ ] T012 Try it in a preview following `specs/011-tab-phase-names/quickstart.md` (`scripts/preview.sh --fresh`; never port 7420), or hand the scenarios to `ui-verifier` with the preview URL.

---

## Dependencies & Execution Order

- **Foundational (T001–T002)** blocks every story.
- **US1 (T003–T004)** and **US3 (T006–T007)** depend only on T002 and touch different files, so they can run in parallel.
- **US2 (T005)** depends on T004 (same file, same code path).
- **Polish**: T008–T010 can start any time after T002 (docs and e2e files only). T011 and T012 come last.

### Parallel Opportunities

```text
After T002:
  US1: T003 → T004   (TaskScreen.tsx, TaskScreen.test.tsx)
  US3: T006 → T007   (TaskView.tsx, TaskView.test.tsx)
  Docs: T008, T009, T010   (docs/usage.md, docs/status.md, app/e2e/*)
```

## Implementation Strategy

- **MVP**: T001–T004 (fixes the reported tab).
- **Then**: T005 (former phase sessions, tests only), T006–T007 (Sessions list), and the polish.
- No daemon, API, migration or version change (FR-007; the version bump happens only when the user asks).
