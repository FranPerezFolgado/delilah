# Work packages — 006 sidebar ideas row

Source: specs/006-sidebar-ideas-row/tasks.md · 17 pending tasks · 6 packages

Phase 1 Setup has no tasks (nothing to do).

## WP-01 ideaToOpen pure function
- Phase: 2 Foundational
- Tasks: T001, T002
- Files: app/src/components/ideas.test.ts, app/src/components/ideas.ts
- Depends on: —
- Parallel group: 2A
- Acceptance: FR-007 — `ideaToOpen(ideas, lastShown)` returns first waiting, else last shown (if listed), else newest (last); tests pass.
- Checks: web: `cd app && npx tsc --noEmit && npx vitest related --run src/components/ideas.ts`
- Reviewers: —

## WP-02 Single Ideas row in the sidebar
- Phase: 3 US1
- Tasks: T003, T004, T005, T006
- Files: app/src/components/Sidebar.test.tsx, app/src/components/Sidebar.tsx, app/src/i18n/en.ts, app/src/App.tsx
- Depends on: —
- Parallel group: 3A
- Acceptance: FR-001, FR-002, FR-003, FR-004, FR-005, FR-008, FR-009 — one "Ideas N" button row first under the project, no row per idea, other sessions unchanged, hidden when folded, project count still includes ideas; App compiles with a temporary handler.
- Checks: web: `cd app && npx tsc --noEmit && npx vitest related --run src/components/Sidebar.tsx src/App.tsx src/i18n/en.ts`
- Reviewers: —

## WP-03 Open the right idea
- Phase: 4 US2
- Tasks: T007, T008, T009
- Files: app/src/components/Sidebar.test.tsx, app/src/components/Sidebar.tsx, app/src/App.tsx
- Depends on: WP-01, WP-02
- Parallel group: 4A
- Acceptance: FR-007 — clicking the row calls `onOpenIdeas(owner, ideas)` in list order and App opens the waiting / last shown / newest idea via `ideaToOpen`.
- Checks: web: `cd app && npx tsc --noEmit && npx vitest related --run src/components/Sidebar.tsx src/App.tsx src/components/ideas.ts`
- Reviewers: —

## WP-04 Waiting highlight on the Ideas count
- Phase: 5 US3
- Tasks: T010, T011
- Files: app/src/components/Sidebar.test.tsx, app/src/components/Sidebar.tsx
- Depends on: WP-02, WP-03
- Parallel group: 5A
- Acceptance: FR-006 — the row's count has `c-wait` iff any idea is waiting.
- Checks: web: `cd app && npx tsc --noEmit && npx vitest related --run src/components/Sidebar.tsx`
- Reviewers: —

## WP-05 Docs
- Phase: 6 Polish
- Tasks: T012, T013, T014
- Files: docs/status.md, README.md, docs/decisions.md
- Depends on: WP-04
- Parallel group: 6A
- Acceptance: status, README and a new numbered decision (dated 2026-10-23) describe the Ideas row and the open order.
- Checks: — (docs only)
- Reviewers: —

## WP-06 Full check, preview, version bump
- Phase: 6 Polish
- Tasks: T015, T016, T017
- Files: Cargo.toml, app/src-tauri/Cargo.toml, app/package.json, app/src-tauri/tauri.conf.json, Cargo.lock, app/src-tauri/Cargo.lock, app/package-lock.json
- Depends on: WP-04, WP-05
- Parallel group: 6B
- Acceptance: all — `npx tsc --noEmit && npx vitest run` green; quickstart scenarios verified in a preview (not on 7420); version 0.6.0 in its own commit `chore: bump version to 0.6.0`.
- Checks: web: `cd app && npx tsc --noEmit && npx vitest run`; `cargo check --workspace`; `cargo check --manifest-path app/src-tauri/Cargo.toml`
- Reviewers: —
