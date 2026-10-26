# Work packages — 003-task-phase-header

Source: specs/003-task-phase-header/tasks.md · 15 pending tasks · 5 packages

Phase 1 Setup has no tasks (nothing to do).

## WP-01 Header phase rule and text
- Phase: 2 Foundational
- Tasks: T001, T002, T003
- Files: app/src/phases.ts, app/src/phases.test.ts, app/src/i18n/en.ts
- Depends on: —
- Parallel group: 2A
- Acceptance: FR-002, FR-004, FR-008 — `headerPhaseOf(task, spec)` returns null for queued/done/no spec, otherwise the board/spec-panel column and its step; "Fase: {phase}" is translated in en.ts.
- Checks: web UI (`cd app && npx tsc --noEmit && npx vitest related --run src/phases.ts`)
- Reviewers: —

## WP-02 Phase badge in the task screen header
- Phase: 3 User Story 1
- Tasks: T004, T005, T006, T007, T008
- Files: app/src/components/PhaseBadge.tsx, app/src/components/PhaseBadge.test.tsx, app/src/styles.css, app/src/components/TaskScreen.tsx, app/src/App.tsx, app/src/components/TaskScreen.test.tsx (guess, only if one exists or is needed)
- Depends on: WP-01
- Parallel group: 3A
- Acceptance: FR-001, FR-002, FR-003, FR-006, FR-007 — the header shows the badge before the status pill, in step with the board and spec panel, updates live, and the existing header content stays intact.
- Checks: web UI (`cd app && npx tsc --noEmit && npx vitest related --run src/components/PhaseBadge.tsx src/components/TaskScreen.tsx src/phases.ts`)
- Reviewers: architecture

## WP-03 Implementation progress count
- Phase: 4 User Story 2
- Tasks: T009, T010, T011, T012
- Files: app/src/phases.ts, app/src/phases.test.ts, app/src/components/PhaseBadge.tsx, app/src/components/PhaseBadge.test.tsx
- Depends on: WP-02
- Parallel group: 4A
- Acceptance: FR-005 — in implement with a task list the badge reads "· done/total" (also in title/aria-label); no count in other phases or with total 0.
- Checks: web UI (`cd app && npx tsc --noEmit && npx vitest related --run src/phases.ts src/components/PhaseBadge.tsx`; then `npx vitest run`)
- Reviewers: —

## WP-04 Docs
- Phase: 5 Polish
- Tasks: T013, T014
- Files: README.md, docs/status.md
- Depends on: WP-02
- Parallel group: 5A
- Acceptance: FR-001, FR-005 — README and docs/status.md describe the phase (and progress) in the task header.
- Checks: — (docs only)
- Reviewers: —

## WP-05 Full check and hand test in a preview
- Phase: 5 Polish
- Tasks: T015
- Files: — (verification only; follows specs/003-task-phase-header/quickstart.md)
- Depends on: WP-03, WP-04
- Parallel group: 5B
- Acceptance: SC-001, SC-002, SC-003, SC-004 — `npx tsc --noEmit && npx vitest run` pass and the five quickstart scenarios hold in `scripts/preview.sh` (never 7420), including 1024 px width.
- Checks: web UI (`cd app && npx tsc --noEmit && npx vitest run`)
- Reviewers: —
