# Work packages — 011-tab-phase-names

Source: specs/011-tab-phase-names/tasks.md · 12 pending tasks · 5 packages

Phase 1 (Setup) has no tasks and is skipped.

## WP-01 sessionName naming rule
- Phase: 2 Foundational
- Tasks: T001, T002
- Files: app/src/chain.ts, app/src/chain.test.ts
- Depends on: —
- Parallel group: 2A
- Acceptance: FR-001..FR-005 — `sessionName(s)` is exported from chain.ts: step set gives the step label (es/en), primary claude without step gives «Claude principal», any other session gives its own label.
- Checks: web UI (`cd app && npx tsc --noEmit && npx vitest related --run src/chain.ts`)
- Reviewers: —

## WP-02 Tab bar names the current phase session
- Phase: 3 User Story 1
- Tasks: T003, T004
- Files: app/src/components/TaskScreen.tsx, app/src/components/TaskScreen.test.tsx
- Depends on: WP-01
- Parallel group: 3A
- Acceptance: FR-001, FR-002, FR-006 — the primary session's tab reads its phase name, the tooltip stays the stored label, primary has no close button, primary without step still reads «Claude principal».
- Checks: web UI (`npx tsc --noEmit && npx vitest related --run app/src/components/TaskScreen.tsx`)
- Reviewers: —

## WP-03 Former phase sessions keep their phase name
- Phase: 4 User Story 2
- Tasks: T005
- Files: app/src/components/TaskScreen.test.tsx, app/src/components/TaskScreen.tsx (only if a test fails)
- Depends on: WP-02
- Parallel group: 4A
- Acceptance: FR-003 — non-primary phase sessions (live or stopped) read their step name, stay closable with «Cerrar <name>», order unchanged; extra Claudes keep their label.
- Checks: web UI (`npx tsc --noEmit && npx vitest related --run app/src/components/TaskScreen.tsx`)
- Reviewers: —

## WP-04 Sessions list uses the same names
- Phase: 5 User Story 3
- Tasks: T006, T007
- Files: app/src/components/TaskView.tsx, app/src/components/TaskView.test.tsx
- Depends on: WP-01
- Parallel group: 5A
- Acceptance: FR-004, SC-003 — `.tv-sname` rows use `sessionName`, equal to the tab labels of TaskScreen.
- Checks: web UI (`npx tsc --noEmit && npx vitest related --run app/src/components/TaskView.tsx`)
- Reviewers: —

## WP-05 Docs, e2e selectors, full checks, preview
- Phase: 6 Polish
- Tasks: T008, T009, T010, T011, T012
- Files: docs/usage.md, docs/status.md, app/e2e/chat.spec.ts, app/e2e/screens.spec.ts
- Depends on: WP-02, WP-03, WP-04
- Parallel group: 6A
- Acceptance: FR-007 — docs describe the new names, e2e selectors still match, full vitest and tsc are green, preview scenarios from quickstart.md verified (never port 7420).
- Checks: web UI (`cd app && npx tsc --noEmit && npx vitest run`)
- Reviewers: —
