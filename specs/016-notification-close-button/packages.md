# Work packages — 016 Close button on every notification

Source: specs/016-notification-close-button/tasks.md · 31 pending tasks · 9 packages

Phase 1 (Setup) has no tasks. All paths are under `app/src/` unless they start with `docs/` or `specs/`. Checks for every package: `cd app && npx tsc --noEmit && npx vitest related --run <files of the package>`.

## WP-01 Shared close control, Notice wrapper and dismissal hooks
- Phase: 2 Foundational
- Tasks: T001, T002, T003, T004
- Files: app/src/components/ui/Notice.test.tsx, app/src/components/ui/Notice.tsx, app/src/components/ui/Notice.css, app/src/components/ui/index.ts
- Depends on: —
- Parallel group: 2A
- Acceptance: FR-002, FR-005, FR-006, FR-007, FR-010, FR-011 — `CloseButton` (name "Cerrar", `.ntc-x`), `Notice`, `useDismissals`, `useDismissal` exported from the kit and passing their tests (hide by id, reappear when the signature changes, focus moves on close)
- Checks: app (tsc --noEmit, vitest related)
- Reviewers: architecture

## WP-02 Global bars: usage limit, autopilot paused, "y N más"
- Phase: 3 US1
- Tasks: T005, T006
- Files: app/src/components/GlobalNotices.test.tsx, app/src/components/GlobalNotices.tsx, app/src/components/GlobalNotices.css (only if the X misaligns)
- Depends on: WP-01
- Parallel group: 3A
- Acceptance: FR-001, FR-003, FR-005, FR-006, FR-007 — each global bar has the X, hides only itself, returns when its signature changes, and triggers no resume or API call
- Checks: app (tsc --noEmit, vitest related)
- Reviewers: —

## WP-03 Update bar in every state
- Phase: 4 US2
- Tasks: T007, T008
- Files: app/src/components/UpdateNotice.test.tsx, app/src/components/UpdateNotice.tsx
- Depends on: WP-01
- Parallel group: 4A
- Acceptance: FR-001, FR-004, FR-005, FR-006 — the X does the existing close-like action (close / later / cancel) or only hides for the in-progress states; the text "Cerrar" buttons are gone; "Más tarde" and "Cancelar" stay
- Checks: app (tsc --noEmit, vitest related)
- Reviewers: —

## WP-04 Toasts use the shared X
- Phase: 5 US3
- Tasks: T009, T010
- Files: app/src/components/ui/Toast.test.tsx, app/src/components/ui/Toast.tsx, app/src/components/ui/Toast.css (guess: stylesheet next to Toast.tsx, only if the 28 px X does not fit)
- Depends on: WP-01
- Parallel group: 5A
- Acceptance: FR-002, FR-004, FR-008 — toast close is `CloseButton`; "Cerrar todo", info auto-close and `onDismiss` behave as before
- Checks: app (tsc --noEmit, vitest related)
- Reviewers: —

## WP-05 Task screen base-stale bars and subtasks banner
- Phase: 6 US4
- Tasks: T011, T012, T019, T020
- Files: app/src/components/TaskScreen.test.tsx, app/src/components/TaskScreen.tsx, app/src/components/SubtaskList.test.tsx, app/src/components/SubtaskList.tsx
- Depends on: WP-01
- Parallel group: 6A
- Acceptance: FR-001, FR-004, FR-005, FR-006 — base-stale X calls `api.dismissNotice` then `onChanged` (hides locally when there is no notice id) and "Descartar" is removed; the Jira subtasks banner closes and returns when the count changes
- Checks: app (tsc --noEmit, vitest related)
- Reviewers: —

## WP-06 Setup, handoff and task view banners
- Phase: 6 US4
- Tasks: T013, T014, T015, T016, T017, T018
- Files: app/src/components/SetupBanner.test.tsx, app/src/components/SetupBanner.tsx, app/src/components/HandoffActions.test.tsx, app/src/components/HandoffActions.tsx, app/src/components/TaskView.test.tsx, app/src/components/TaskView.tsx
- Depends on: WP-01
- Parallel group: 6A
- Acceptance: FR-001, FR-003, FR-005, FR-006, FR-007 — setup (running/failed), autopilot-paused and the four `tv-notice` bars have the X, hide only themselves with no API call, and reappear when their signature changes
- Checks: app (tsc --noEmit, vitest related)
- Reviewers: —

## WP-07 PR view notices and Jira expired banner
- Phase: 6 US4
- Tasks: T021, T022, T023, T024
- Files: app/src/components/PrView.test.tsx, app/src/components/PrView.tsx, app/src/components/JiraView.test.tsx, app/src/components/JiraView.tsx
- Depends on: WP-01
- Parallel group: 6A
- Acceptance: FR-001, FR-005, FR-006, FR-007 — the four PR notices (per PR) and the Jira 401 banner have the X; their own buttons stay; the Jira list keeps `stale` after the banner closes
- Checks: app (tsc --noEmit, vitest related)
- Reviewers: —

## WP-08 Disconnected banner
- Phase: 7 US5
- Tasks: T025, T026
- Files: app/src/components/DisconnectedBanner.test.tsx, app/src/components/DisconnectedBanner.tsx, app/src/App.test.tsx, app/src/App.tsx
- Depends on: WP-01
- Parallel group: 7A
- Acceptance: FR-001, FR-009 — the X hides the banner but `.shell-content` stays inert/disabled while offline; the banner returns on the next disconnection
- Checks: app (tsc --noEmit, vitest related)
- Reviewers: —

## WP-09 Polish, docs and full verification
- Phase: 8 Polish
- Tasks: T027, T028, T029, T030, T031
- Files: docs/usage.md, docs/status.md, docs/decisions.md, app/src/ (grep sweep for leftover close controls; files unknown, guess: none or one of the N1–N12 components)
- Depends on: WP-02, WP-03, WP-04, WP-05, WP-06, WP-07, WP-08
- Parallel group: 8A
- Acceptance: FR-001, FR-002, FR-004 — no leftover text close buttons in notifications; docs updated; `cd app && npx tsc --noEmit && npx vitest run` green; scenarios tried in a preview (never 7420)
- Checks: app (tsc --noEmit, vitest run)
- Reviewers: —
