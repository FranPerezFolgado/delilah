# Work packages — 008 Group toasts

Source: specs/008-group-toasts/tasks.md · 27 pending tasks · 10 packages

Checks (web UI): `cd app && npx tsc --noEmit && npx vitest related --run <files>`; at the end `npx vitest run`.

## WP-01 Setup: Toast i18n file
- Phase: 1 Setup
- Tasks: T001
- Files: app/src/i18n/en/Toast.ts
- Depends on: —
- Parallel group: 1A
- Acceptance: setup — `EN` export exists (empty record) and is picked up by the `./en/*.ts` glob; i18n coverage test still green
- Checks: web UI (tsc, vitest related, src/i18n/coverage.test.ts)
- Reviewers: —

## WP-02 Toast store: keys, timers, eviction
- Phase: 2 Foundational
- Tasks: T002, T003, T004, T005, T006
- Files: app/src/components/ui/Toast.test.tsx, app/src/components/ui/Toast.tsx, app/src/components/ui/index.ts
- Depends on: WP-01
- Parallel group: 2A
- Acceptance: foundation for FR-001–FR-003 and later stories — keyed in-place update, `dismissToastByKey`, `WAITING_TOAST_KEY`, `onDismiss` only on ×, store-owned timers with pause API, eviction never drops keyed toasts; existing call sites compile unchanged
- Checks: web UI (tsc, vitest related Toast.tsx)
- Reviewers: architecture

## WP-03 US1 grouped «te espera» toast
- Phase: 3 US1
- Tasks: T007, T008, T009, T010
- Files: app/src/components/GlobalNotices.test.tsx, app/src/components/GlobalNotices.tsx, app/src/App.tsx, app/src/i18n/en/GlobalNotices.ts
- Depends on: WP-02
- Parallel group: 3A
- Acceptance: US1 (FR-001–FR-003) — at most one waiting toast («X te espera · Responder» / «N tareas te esperan · Ver»), updated in place, removed when no task waits; `onOpenInbox` wired in App
- Checks: web UI (tsc, vitest related GlobalNotices.tsx App.tsx, i18n coverage)
- Reviewers: —

## WP-04 US2 merge repeated toasts
- Phase: 4 US2
- Tasks: T011, T012, T013
- Files: app/src/components/ui/Toast.test.tsx, app/src/components/ui/Toast.tsx, app/src/components/ui/Toast.css, app/src/i18n/en/Toast.ts
- Depends on: WP-02
- Parallel group: 4A
- Acceptance: US2 — same kind+text, no action, no key merges into one toast with «×N» (cap «×99+»), moved to newest, ttl restarted
- Checks: web UI (tsc, vitest related Toast.tsx, i18n coverage)
- Reviewers: —

## WP-05 US3 collapsed stack
- Phase: 5 US3
- Tasks: T014, T015, T016, T017
- Files: app/src/components/ui/Toast.test.tsx, app/src/components/ui/Toast.tsx, app/src/components/ui/Toast.css, app/src/i18n/en/Toast.ts
- Depends on: WP-04
- Parallel group: 5A
- Acceptance: US3 — newest toast in full plus «+N»; hover/focus/click expands; «Cerrar todo» closes all calling `onDismiss`; single hidden `role="status"` announcer
- Checks: web UI (tsc, vitest related Toast.tsx, i18n coverage)
- Reviewers: architecture

## WP-06 US4 pause timers while expanded
- Phase: 6 US4
- Tasks: T018, T019
- Files: app/src/components/ui/Toast.test.tsx, app/src/components/ui/Toast.tsx
- Depends on: WP-05
- Parallel group: 6A
- Acceptance: US4 — info timers pause while stack is hovered/focused/expanded and resume with remaining time; collapsed hidden toasts keep expiring
- Checks: web UI (tsc, vitest related Toast.tsx)
- Reviewers: —

## WP-07 US4 undo toast tied to pending discard
- Phase: 6 US4
- Tasks: T020, T021
- Files: app/src/components/ProposalCard.test.tsx, app/src/components/ProposalCard.tsx
- Depends on: WP-02
- Parallel group: 6A
- Acceptance: US4 — «Tarea descartada» toast is dismissed by id when the discard timer fires, even if the stack was hovered
- Checks: web UI (tsc, vitest related ProposalCard.tsx)
- Reviewers: —

## WP-08 Polish: full suite and fallout fixes
- Phase: 7 Polish
- Tasks: T022
- Files: app/src/App.test.tsx, app/src/components/TaskView.test.tsx, app/src/components/InboxView.test.tsx, app/src/components/BacklogTaskView.test.tsx, app/src/components/TaskScreen.test.tsx (guess: only those that fail)
- Depends on: WP-03, WP-06, WP-07
- Parallel group: 7A
- Acceptance: all FRs — `npx tsc --noEmit && npx vitest run` green; tests adapted off per-toast `role="status"` and per-task «te espera» toasts without weakening behaviour assertions
- Checks: web UI (full: tsc, vitest run)
- Reviewers: —

## WP-09 Polish: docs
- Phase: 7 Polish
- Tasks: T023, T024, T025
- Files: README.md, docs/status.md, docs/architecture.md, docs/decisions.md
- Depends on: WP-03, WP-06, WP-07
- Parallel group: 7A
- Acceptance: user-facing behaviour, status, architecture note and decision (grouped waiting toast, merge rule, collapsed stack, ruled-out alternatives) documented
- Checks: none (docs)
- Reviewers: —

## WP-10 Polish: preview verification and version bump
- Phase: 7 Polish
- Tasks: T026, T027
- Files: app/package.json, app/src-tauri/Cargo.toml, app/src-tauri/tauri.conf.json, Cargo.toml, lockfiles (guess: the four places in docs/development.md#releasing, via scripts/bump-version.sh minor --commit)
- Depends on: WP-08, WP-09
- Parallel group: 7B
- Acceptance: quickstart scenarios 1–5 verified in a preview (never 7420); version bumped minor in its own commit `chore: bump version to X.Y.Z`
- Checks: web UI (full), preview via scripts/preview.sh --fresh (ui-verifier)
- Reviewers: —
