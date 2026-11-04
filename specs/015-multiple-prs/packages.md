# Work packages — 015-multiple-prs

Source: specs/015-multiple-prs/tasks.md · 48 pending tasks · 20 packages

Delivery (from tasks.md): PR 1 = WP-01..07, PR 2 = WP-08..14, PR 3 = WP-15..20.

## WP-01 Fixtures and test helpers
- Phase: 1 Setup
- Tasks: T001, T002
- Files: crates/delilah-daemon/tests/fixtures/hooks/post_tool_use_gh_pr_create.json, crates/delilah-daemon/tests/fixtures/hooks/post_tool_use_gh_pr_create_exists.json, specs/015-multiple-prs/research.md, crates/delilah-daemon/src/test_support.rs (guess)
- Depends on: —
- Parallel group: 1A
- Acceptance: FR-004 — real PostToolUse fixtures recorded and a parent workspace helper (svc-a, svc-b, fake gh per repo folder) available to later tests
- Checks: cargo test -p delilah-daemon
- Reviewers: —

## WP-02 Migration 0027 and per-URL store
- Phase: 2 Foundational
- Tasks: T003, T004, T009, T011
- Files: crates/delilah-daemon/migrations/0027_pr_per_url.sql, crates/delilah-daemon/src/store_prs.rs
- Depends on: WP-01
- Parallel group: 2A
- Acceptance: SC-004 — pull_requests keyed by (task_key, url) with replaced_by, pr_notices with pr_url, data survives migration, link/unlink per PR, counted() helper
- Checks: cargo test -p delilah-daemon
- Reviewers: migration

## WP-03 Snapshot head_branch
- Phase: 2 Foundational
- Tasks: T005, T010
- Files: crates/delilah-daemon/src/pr_types.rs, crates/delilah-daemon/src/gh/graphql.rs, crates/delilah-daemon/src/gh/mod.rs, crates/delilah-daemon/src/gh/fake.rs
- Depends on: —
- Parallel group: 2A
- Acceptance: FR-009 — snapshot carries head_branch (parsed from headRefName); old stored JSON still deserialises
- Checks: cargo test -p delilah-daemon
- Reviewers: —

## WP-04 Notices, forward and inbox by PR URL
- Phase: 2 Foundational
- Tasks: T006, T012, T013
- Files: crates/delilah-daemon/src/pr_loop/notices.rs, crates/delilah-daemon/src/pr_loop/mod.rs, crates/delilah-daemon/src/pr_api/forward.rs, crates/delilah-daemon/src/inbox.rs
- Depends on: WP-02
- Parallel group: 2B
- Acceptance: FR-001 — notices are matched to their own PR by pr_url; forward and inbox items act on the right PR when two share a repo
- Checks: cargo test -p delilah-daemon
- Reviewers: architecture

## WP-05 Autofix per PR and subtask rollup
- Phase: 2 Foundational
- Tasks: T007, T008, T014, T015
- Files: crates/delilah-daemon/src/autofix/mod.rs, crates/delilah-daemon/src/api.rs, crates/delilah-daemon/src/integration.rs
- Depends on: WP-02
- Parallel group: 2B
- Acceptance: FR-001 — autofix retry/cancel/stop act on one PR (`pr: <id>` accepted, `repo` still works); replaced PRs ignored in subtask_facts
- Checks: cargo test -p delilah-daemon
- Reviewers: contract

## WP-06 Repo-lookup sweep and PR DTO
- Phase: 2 Foundational
- Tasks: T016, T017
- Files: crates/delilah-daemon/src (sweep of `.repo ==`, `repo = ?`, `r.repo`), crates/delilah-daemon/src/store.rs, crates/delilah-daemon/src/jira_import.rs, crates/delilah-daemon/src/pr_api/dto.rs, crates/delilah-daemon/src/prs.rs
- Depends on: WP-02, WP-03, WP-04, WP-05
- Parallel group: 2C
- Acceptance: FR-001 — no remaining "the PR of a repo" lookup; DTO exposes replaced_by, head_branch, linked_by "agent", ordered by repo then id; summary over counted PRs
- Checks: cargo test -p delilah-daemon
- Reviewers: contract

## WP-07 App types and PR cards by id
- Phase: 2 Foundational
- Tasks: T018, T019
- Files: app/src/api.ts, app/src/components/PrView.tsx, app/src/components/PrView.test.tsx
- Depends on: WP-06
- Parallel group: 2D
- Acceptance: FR-001 — api.ts mirrors the contract; two PRs of one repo render two cards and retry sends the PR id
- Checks: app (tsc + vitest)
- Reviewers: contract

## WP-08 pr_repos and detection
- Phase: 3 User Story 1
- Tasks: T020, T021, T024, T025
- Files: crates/delilah-git/src/workspace.rs, crates/delilah-daemon/src/pr_loop/mod.rs
- Depends on: WP-01, WP-06
- Parallel group: 3A
- Acceptance: FR-001, FR-002 — Workspace::pr_repos adds `.` for a parent on the task branch; detection searches it and keeps searching repos with only closed/other-branch PRs
- Checks: cargo test -p delilah-git, cargo test -p delilah-daemon
- Reviewers: architecture

## WP-09 Replacement of closed PRs
- Phase: 3 User Story 1
- Tasks: T023, T026
- Files: crates/delilah-daemon/src/store_prs.rs
- Depends on: WP-06
- Parallel group: 3A
- Acceptance: FR-009 — a new PR with the same repo and head branch as a closed one marks it replaced_by and resolves its notices; open/merged never replaced
- Checks: cargo test -p delilah-daemon
- Reviewers: —

## WP-10 Capture agent-created PRs
- Phase: 3 User Story 1
- Tasks: T022, T027, T028
- Files: crates/delilah-daemon/src/pr_loop/capture.rs, crates/delilah-daemon/src/pr_loop/mod.rs, crates/delilah-daemon/src/phases.rs
- Depends on: WP-08, WP-09
- Parallel group: 3B
- Acceptance: FR-003, FR-004 — PR URLs from `gh pr create` stdout/stderr in PostToolUse Bash hooks are linked with linked_by "agent"; other commands ignored; no duplicates
- Checks: cargo test -p delilah-daemon
- Reviewers: security, architecture

## WP-11 UI source and outside-repo labels
- Phase: 3 User Story 1
- Tasks: T029
- Files: app/src/components/PrView.tsx, app/src/components/PrView.test.tsx, app/src/i18n/en.ts, app/src/i18n/es.ts (guess)
- Depends on: WP-10
- Parallel group: 3C
- Acceptance: FR-018 — outside repos grouped under host/owner/name and "agent" source shown, strings in both languages
- Checks: app (tsc + vitest)
- Reviewers: —

## WP-12 Link by URL for any repo
- Phase: 4 User Story 3
- Tasks: T030, T033
- Files: crates/delilah-daemon/src/pr_api/mod.rs, crates/delilah-daemon/src/pr_loop/mod.rs
- Depends on: WP-08, WP-10
- Parallel group: 4A
- Acceptance: FR-005 — repo_of resolves `.`, submodules, else host/owner/name; NotFromTask/409 removed; unlinking one of two PRs leaves the other
- Checks: cargo test -p delilah-daemon
- Reviewers: security, contract

## WP-13 Open PR from the parent repo
- Phase: 4 User Story 3
- Tasks: T031, T034
- Files: crates/delilah-daemon/src/pr_api/open.rs
- Depends on: WP-08
- Parallel group: 4A
- Acceptance: FR-006 — proposal repos include `.` while it has no open PR from the task branch; open with `.` pushes and creates from the parent folder; unknown repo 400
- Checks: cargo test -p delilah-daemon
- Reviewers: contract, security

## WP-14 Open PR dialog offers the parent repo
- Phase: 4 User Story 3
- Tasks: T032, T035
- Files: app/src/components/OpenPrDialog.tsx, app/src/components/OpenPrDialog.test.tsx
- Depends on: WP-13
- Parallel group: 4B
- Acceptance: FR-006 — the repo select offers `.` labelled as the parent repository
- Checks: app (tsc + vitest)
- Reviewers: —

## WP-15 Merge rule and archive heads
- Phase: 5 User Story 2
- Tasks: T036, T038, T040, T042
- Files: crates/delilah-daemon/src/pr_loop/merge.rs
- Depends on: WP-08
- Parallel group: 5A
- Acceptance: FR-010, FR-013 — all_merged over counted PRs only; archive heads built per label from merged task-branch PRs, parent branch deleted only with a merged parent PR
- Checks: cargo test -p delilah-daemon
- Reviewers: —

## WP-16 Pre-archive detection and no mid-turn archive
- Phase: 5 User Story 2
- Tasks: T037, T041
- Files: crates/delilah-daemon/src/pr_loop/mod.rs
- Depends on: WP-15
- Parallel group: 5B
- Acceptance: FR-011, FR-012 — one forced detection before archiving; no archive while the primary session is Working
- Checks: cargo test -p delilah-daemon
- Reviewers: —

## WP-17 Nudge PR loop on turn end
- Phase: 5 User Story 2
- Tasks: T039, T043
- Files: crates/delilah-daemon/src/phases.rs
- Depends on: WP-10
- Parallel group: 5A
- Acceptance: FR-012 — TurnEnded on a task with PRs nudges the PR loop
- Checks: cargo test -p delilah-daemon
- Reviewers: —

## WP-18 UI replaced PRs
- Phase: 5 User Story 2
- Tasks: T044
- Files: app/src/components/PrView.tsx, app/src/components/PrView.test.tsx, app/src/i18n/en.ts, app/src/i18n/es.ts (guess)
- Depends on: WP-11
- Parallel group: 5A
- Acceptance: FR-009 — replaced PRs shown muted with "replaced" and left out of the badge count
- Checks: app (tsc + vitest)
- Reviewers: —

## WP-19 Docs
- Phase: 6 Polish
- Tasks: T045, T046
- Files: docs/usage.md, docs/status.md, docs/architecture.md, docs/decisions.md
- Depends on: WP-16, WP-17, WP-18, WP-14, WP-12
- Parallel group: 6A
- Acceptance: user-facing behaviour, built state, data model and the new decision documented
- Checks: —
- Reviewers: —

## WP-20 Full checks and version bump
- Phase: 6 Polish
- Tasks: T047, T048
- Files: Cargo.toml, app/src-tauri/Cargo.toml, app/package.json, app/src-tauri/tauri.conf.json, CHANGELOG.md, Cargo.lock
- Depends on: WP-19
- Parallel group: 6B
- Acceptance: all checks green, quickstart steps 1-7 walked in a preview, patch bump in its own commit
- Checks: cargo test --workspace, cargo test --manifest-path app/src-tauri/Cargo.toml, app (tsc + vitest run)
- Reviewers: —
