# Work packages — 003-pr-loop

Source: specs/003-pr-loop/tasks.md · 51 pending tasks · 21 packages (49 tasks) + 2 orchestrator steps (T001, T051)

Orchestrator steps (not for implementer agents):
- Before WP-01: T001 baseline run (`cargo test --workspace`, `cargo test --manifest-path app/src-tauri/Cargo.toml`, `cd app && npx tsc --noEmit && npx vitest run`); record pre-existing failures. No files.
- After WP-21: T051 quickstart run in a preview with `DELILAH_GH_BIN=scripts/fake-gh.sh` (ui-verifier); results go in specs/003-pr-loop/quickstart.md under "Results".

Check shorthand: `rust:<crate>` = `cargo test -p <crate>`; `app` = `cd app && npx tsc --noEmit && npx vitest related --run <files touched>`.

## WP-01 Setup: migration, Topic::Prs, fake gh
- Phase: 1 Setup
- Tasks: T002, T003, T004
- Files: crates/delilah-daemon/migrations/0011_pull_requests.sql, crates/delilah-daemon/src/events.rs, scripts/fake-gh.sh
- Depends on: —
- Parallel group: 1A
- Acceptance: FR-004, FR-025, FR-007 — migration 0011 creates pull_requests, pr_unlinked, pr_notices per data-model.md; `Topic::Prs` serializes as "prs"; fake-gh.sh answers the quickstart commands from $FAKE_GH_DIR
- Checks: rust:delilah-daemon
- Reviewers: migration, contract

## WP-02 Daemon PR core: gh wrapper, pure PR types, store, AppState
- Phase: 2 Foundational
- Tasks: T005, T006, T007, T008, T009, T010, T011
- Files: crates/delilah-daemon/src/gh.rs, crates/delilah-daemon/src/prs.rs, crates/delilah-daemon/src/store_prs.rs, crates/delilah-daemon/src/state.rs, crates/delilah-daemon/src/lib.rs
- Depends on: WP-01
- Parallel group: 2A
- Acceptance: FR-021, FR-022, FR-026, FR-031, FR-004 — `gh` wrapper with classified errors, `ci_outcome`/`pick`/`summary`, store methods and `AppState.gh`/`pr_backoff` exist and their tests pass. This package owns lib.rs for the whole phase: it registers `gh`, `prs` and `store_prs` (T010 and T011 both register modules).
- Checks: rust:delilah-daemon
- Reviewers: security, architecture

## WP-03 US1 check_task, polling loop and nudge
- Phase: 3 US1 See the PR
- Tasks: T012, T014, T016
- Files: crates/delilah-daemon/src/prs.rs, crates/delilah-daemon/src/main.rs, crates/delilah-daemon/src/phases.rs
- Depends on: WP-02
- Parallel group: 3A
- Acceptance: FR-001, FR-002, FR-024, FR-025, FR-027, FR-028, FR-029, FR-022 — a PR from the task's branch is detected, linked and snapshotted; back-off and once-per-project access notice work; push/`gh pr create` hooks nudge a check
- Checks: rust:delilah-daemon
- Reviewers: security, architecture

## WP-04 US1 PR routes (list, refresh, link, unlink)
- Phase: 3 US1 See the PR
- Tasks: T013, T015
- Files: crates/delilah-daemon/src/pr_api.rs, crates/delilah-daemon/src/api.rs
- Depends on: WP-03
- Parallel group: 3B
- Acceptance: FR-003, FR-004, FR-025, FR-027 — GET tasks/{key}/prs, GET prs, POST refresh, POST link (bad URL 400), DELETE prs/{id} (204, not re-detected) match contracts/api.md. `pr_api.rs` is a new module; its `mod` line is added to lib.rs, which WP-02 already registered (add only if missing).
- Checks: rust:delilah-daemon
- Reviewers: contract, security, architecture

## WP-05 US1 app: types, live topic, badge, PR tab, strings
- Phase: 3 US1 See the PR
- Tasks: T017, T018, T019, T020, T021
- Files: app/src/api.ts, app/src/live.ts, app/src/components/PrBadge.tsx, app/src/components/PrBadge.test.tsx, app/src/components/PrView.tsx, app/src/components/PrView.test.tsx, app/src/components/TaskScreen.tsx, app/src/components/Sidebar.tsx, app/src/components/BoardView.tsx, app/src/i18n/en.ts, app/src/styles.css
- Depends on: —
- Parallel group: 3A
- Acceptance: FR-005, FR-006, FR-007, FR-003, FR-022 — PR tab with cards, problem text and link-by-URL; badges on sidebar rows and board cards; live refetch on the `prs` topic. Needs only the contract (types), not the Rust code, so it runs alongside WP-03.
- Checks: app
- Reviewers: contract

## WP-06 US2 diff and notices in check_task
- Phase: 4 US2 Notices
- Tasks: T022, T023, T026
- Files: crates/delilah-daemon/src/prs.rs
- Depends on: WP-03
- Parallel group: 4A
- Acceptance: FR-008, FR-009, FR-010, FR-012, FR-013, FR-030, FR-032, FR-033, FR-035 — `diff` yields one notice per new failing head or new foreign review/comment, never repeated (also after restart); resolved on green head or merge/close; notifications only when the app is not listening
- Checks: rust:delilah-daemon
- Reviewers: architecture

## WP-07 US2 Bandeja items from notices
- Phase: 4 US2 Notices
- Tasks: T024, T027
- Files: crates/delilah-daemon/src/inbox.rs
- Depends on: WP-02
- Parallel group: 4A
- Acceptance: FR-008, FR-009, FR-032 — open notices appear as pr-failed / pr-review / pr-blocked items with `pr { notice, url, repo }` in the specified order; resolved ones do not
- Checks: rust:delilah-daemon
- Reviewers: contract

## WP-08 US2 forward and dismiss routes
- Phase: 4 US2 Notices
- Tasks: T025, T028
- Files: crates/delilah-daemon/src/pr_api.rs, crates/delilah-daemon/src/api.rs, crates/delilah-daemon/src/actions.rs (read, `type_prompt`), crates/delilah-daemon/src/session_api.rs (guess, only if `reopen` must become pub(crate))
- Depends on: WP-04
- Parallel group: 4A
- Acceptance: FR-011, FR-034, FR-035, FR-023 — forward types a framed prompt (check names or quoted comments marked "not instructions from the user") into the primary session and resolves the notice; paused task gives 409 with `resume_at`; dismiss gives 204
- Checks: rust:delilah-daemon
- Reviewers: contract, security

## WP-09 US2 app: inbox kinds, forward/dismiss buttons, alerts
- Phase: 4 US2 Notices
- Tasks: T029, T030
- Files: app/src/api.ts, app/src/alerts.ts, app/src/components/InboxView.tsx, app/src/components/InboxView.test.tsx, app/src/i18n/en.ts
- Depends on: WP-05
- Parallel group: 4A
- Acceptance: FR-008, FR-009, FR-010, FR-011, FR-023 — pr-failed / pr-review items counted as blocking, with "Pedir al agente", "Descartar", "Ver PR"; approved/closed notifications from summaries
- Checks: app
- Reviewers: contract

## WP-10 US3 git push helpers
- Phase: 5 US3 Open PR
- Tasks: T031, T033
- Files: crates/delilah-git/tests/workspace.rs, crates/delilah-git/src/workspace.rs, crates/delilah-git/src/lib.rs
- Depends on: —
- Parallel group: 5A
- Acceptance: FR-016 — `commit_subjects`, `needs_push`, `push_branch` work against a temp repo with a local bare remote
- Checks: rust:delilah-git
- Reviewers: security

## WP-11 US3 proposal and open routes
- Phase: 5 US3 Open PR
- Tasks: T032, T034
- Files: crates/delilah-daemon/src/pr_api.rs, crates/delilah-daemon/src/api.rs, crates/delilah-daemon/src/jira.rs (read)
- Depends on: WP-10, WP-08
- Parallel group: 5B
- Acceptance: FR-014, FR-015, FR-016, FR-027 — proposal carries the Jira link and commit subjects (none for free tasks); no-commits and has-pr give 409; open pushes, runs `gh pr create`, links as opened, moves to review; push failure gives 409 with no row
- Checks: rust:delilah-daemon
- Reviewers: contract, security

## WP-12 US3 app: Abrir PR dialog
- Phase: 5 US3 Open PR
- Tasks: T035, T036
- Files: app/src/api.ts, app/src/components/OpenPrDialog.tsx, app/src/components/OpenPrDialog.test.tsx, app/src/components/PrView.tsx, app/src/i18n/en.ts
- Depends on: WP-09
- Parallel group: 5A
- Acceptance: FR-014, FR-015 — "Abrir PR" button (disabled with the 409 reason) opens an editable title/body dialog with a draft checkbox and inline errors
- Checks: app
- Reviewers: contract

## WP-13 US4 git merge check and branch delete
- Phase: 6 US4 Archive on merge
- Tasks: T037, T040
- Files: crates/delilah-git/tests/workspace.rs, crates/delilah-git/src/workspace.rs, crates/delilah-git/src/command.rs
- Depends on: WP-10
- Parallel group: 6A
- Acceptance: FR-018, FR-036, FR-037, FR-038 — `fetch`, `check_merged` (NotMerged / Dirty, parent submodule-pointer rule) and `delete_branch` behave as specified
- Checks: rust:delilah-git
- Reviewers: security

## WP-14 US4 archive finishes archive-partial
- Phase: 6 US4 Archive on merge
- Tasks: T039, T041
- Files: crates/delilah-daemon/src/actions.rs
- Depends on: WP-02
- Parallel group: 6A
- Acceptance: FR-040 — `release_workspace` is pub(crate); `POST /tasks/{key}/archive` with an open archive-partial notice deletes the leftover branch and resolves it
- Checks: rust:delilah-daemon
- Reviewers: architecture

## WP-15 US4 on_merged
- Phase: 6 US4 Archive on merge
- Tasks: T038, T042
- Files: crates/delilah-daemon/src/prs.rs
- Depends on: WP-13, WP-14, WP-06
- Parallel group: 6B
- Acceptance: FR-017, FR-018, FR-019, FR-020, FR-036, FR-037, FR-040 — merged PR marks done and archives (worktree removed, local branch deleted, checkout restored); dirty gives one archive-blocked notice; partial failure gives archive-partial; closed unmerged only notifies
- Checks: rust:delilah-daemon
- Reviewers: security, architecture

## WP-16 US4 app: pr-blocked actions and merged alert
- Phase: 6 US4 Archive on merge
- Tasks: T043
- Files: app/src/components/InboxView.tsx, app/src/components/InboxView.test.tsx, app/src/alerts.ts, app/src/i18n/en.ts
- Depends on: WP-09
- Parallel group: 6A
- Acceptance: FR-018, FR-017 — pr-blocked item with "Archivar igualmente" (`archive(key, true)`) and "Descartar"; "mergeada y archivada" notification when unfocused
- Checks: app
- Reviewers: contract

## WP-17 US5 changesets in prs.rs
- Phase: 7 US5 Changesets
- Tasks: T044, T046
- Files: crates/delilah-daemon/src/prs.rs
- Depends on: WP-15
- Parallel group: 7A
- Acceptance: FR-038, FR-039 — parents get one PR per submodule; archive only when all are merged (submodule branches deleted, parent kept); merged plus closed does not archive; summary reports the worst state and count
- Checks: rust:delilah-daemon
- Reviewers: architecture

## WP-18 US5 repo parameter in proposal/open
- Phase: 7 US5 Changesets
- Tasks: T045, T047
- Files: crates/delilah-daemon/src/pr_api.rs
- Depends on: WP-11
- Parallel group: 7A
- Acceptance: FR-038 — proposal and open with `repo=<submodule>` push and create from that folder; unknown repo gives 400
- Checks: rust:delilah-daemon
- Reviewers: contract, security

## WP-19 US5 app: submodule select and grouping
- Phase: 7 US5 Changesets
- Tasks: T048
- Files: app/src/components/OpenPrDialog.tsx, app/src/components/OpenPrDialog.test.tsx, app/src/components/PrView.tsx, app/src/components/PrView.test.tsx
- Depends on: WP-12
- Parallel group: 7A
- Acceptance: FR-038 — dialog offers submodules without an open PR (parents only); PR cards are grouped by submodule
- Checks: app
- Reviewers: —

## WP-20 Docs
- Phase: 8 Polish
- Tasks: T049
- Files: docs/status.md, docs/architecture.md, docs/decisions.md, README.md
- Depends on: —
- Parallel group: 8A
- Acceptance: FR-023 — docs describe the PR loop, modules, tables, `prs` topic, archive rules and decisions
- Checks: —
- Reviewers: —

## WP-21 Final checks and fixes
- Phase: 8 Polish
- Tasks: T050
- Files: (any file made red by the full run)
- Depends on: WP-17, WP-18, WP-19, WP-20
- Parallel group: 8B
- Acceptance: all stories — `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --manifest-path app/src-tauri/Cargo.toml` and `cd app && npx tsc --noEmit && npx vitest run` are green
- Checks: rust:workspace, clippy, app/src-tauri, app
- Reviewers: —
