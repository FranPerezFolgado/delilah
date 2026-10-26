# Work packages — 008-autopilot-background-stall

Source: specs/008-autopilot-background-stall/tasks.md · 20 pending tasks · 10 packages

No version bump on this branch (tasks.md; the user releases on request).

## WP-01 Hook payload fixtures
- Phase: 1 Setup
- Tasks: T001
- Files: crates/delilah-daemon/tests/fixtures/hooks/stop-background-running.json, crates/delilah-daemon/tests/fixtures/hooks/stop-background-done.json, crates/delilah-daemon/tests/fixtures/hooks/stop-background-empty.json, crates/delilah-daemon/tests/fixtures/hooks/stop-no-background-field.json
- Depends on: —
- Parallel group: 1A
- Acceptance: FR-008, FR-009 — four neutral Stop payload fixtures exist in the shape of contracts/stop-hook.md, with no real content.
- Checks: none (JSON only; exercised by WP-05 and WP-06)
- Reviewers: —

## WP-02 background_running payload rule
- Phase: 2 Foundational
- Tasks: T002, T003
- Files: crates/delilah-agents/src/claude.rs
- Depends on: —
- Parallel group: 2A
- Acceptance: FR-001, FR-003 — `background_running(&Value) -> bool` is true only when a `background_tasks` entry has `status == "running"`; every other shape is false.
- Checks: cargo test -p delilah-agents, cargo clippy -p delilah-agents --all-targets -- -D warnings
- Reviewers: —

## WP-03 Fake Claude: stop object form, write step, per-command scenarios
- Phase: 2 Foundational
- Tasks: T004, T005, T006, T007
- Files: crates/delilah-e2e/src/fake/scenario.rs, crates/delilah-e2e/src/fake/hooks.rs, crates/delilah-e2e/src/fake/run.rs
- Depends on: —
- Parallel group: 2A
- Acceptance: FR-006 — the fake can end a turn with `background_tasks`, write files under its cwd (rejecting absolute/`..`/empty paths with exit 2) and selects `<task>.<command>.json` before `<task>.json`.
- Checks: cargo test -p delilah-e2e --lib, cargo clippy -p delilah-e2e --all-targets -- -D warnings
- Reviewers: security

## WP-04 Fake-claude contract doc update
- Phase: 2 Foundational
- Tasks: T008
- Files: specs/005-e2e-tests/contracts/fake-claude.md
- Depends on: —
- Parallel group: 2A
- Acceptance: FR-006 — the 005 contract lists the lookup order, the `stop` object form, the `write` step and `background_tasks` in the Stop payload row, as in 008's contracts/fake-claude.md.
- Checks: none (docs)
- Reviewers: —

## WP-05 Skip the stall while background work runs (tests + fix + e2e)
- Phase: 3 US1
- Tasks: T009, T010, T011, T012
- Files: crates/delilah-daemon/src/hooks.rs, crates/delilah-daemon/src/autopilot/testkit.rs (guess, for the post_hook helper), crates/delilah-e2e/tests/autopilot.rs, crates/delilah-e2e/src/api.rs (only if history isn't reachable), crates/delilah-daemon/src/handoff.rs, crates/delilah-daemon/src/phases.rs, specs/008-autopilot-background-stall/progress.md
- Depends on: WP-01, WP-02, WP-03
- Parallel group: 3A
- Acceptance: FR-001, FR-002, FR-004, SC-001, SC-004 — a primary-session turn end with a running background task causes no nudge, pause or event; the step-end handoff still runs; the e2e run reaches review without pause or nudge. Tests are seen failing before T011-T012 (output noted in progress.md).
- Checks: cargo test -p delilah-daemon, cargo test -p delilah-e2e --test autopilot, cargo clippy --workspace --all-targets -- -D warnings
- Reviewers: architecture

## WP-06 Daemon regression guards for a real stall
- Phase: 4 US2
- Tasks: T013
- Files: crates/delilah-daemon/src/hooks.rs, crates/delilah-daemon/src/autopilot/testkit.rs (guess)
- Depends on: WP-05
- Parallel group: 4A
- Acceptance: FR-003, FR-005, FR-008, SC-002 — finished/empty/absent background lists nudge once then pause; running then empty nudges; concurrent running+empty gives one nudge; a StopFailure with a running entry still pauses.
- Checks: cargo test -p delilah-daemon
- Reviewers: —

## WP-07 E2E: a real stall still nudges then pauses
- Phase: 4 US2
- Tasks: T014
- Files: crates/delilah-e2e/tests/autopilot.rs
- Depends on: WP-05
- Parallel group: 4A
- Acceptance: FR-003, FR-007, SC-002 — one `autopilot-nudged` then `autopilot-paused`, state `paused` with the unchecked-tasks reason, and the nudge text appears in the fake's record.
- Checks: cargo test -p delilah-e2e --test autopilot
- Reviewers: —

## WP-08 Prove the tests can fail; fixture hygiene
- Phase: 5 US3
- Tasks: T015, T016
- Files: specs/008-autopilot-background-stall/progress.md, crates/delilah-daemon/tests/fixtures/hooks/ (grep only, fix on hit), crates/delilah-e2e/tests/autopilot.rs (grep only, fix on hit)
- Depends on: WP-06, WP-07
- Parallel group: 5A
- Acceptance: FR-009, SC-003 — autopilot_waits_on_background_work fails without T011-T012 and passes on head (both recorded); no real-content strings in fixtures or scenarios.
- Checks: cargo test -p delilah-e2e --test autopilot
- Reviewers: —

## WP-09 Docs: decision #39, status, README
- Phase: 6 Polish
- Tasks: T017, T018, T019
- Files: docs/decisions.md, docs/status.md, README.md
- Depends on: WP-05
- Parallel group: 6A
- Acceptance: FR-010 — decision #39, status and README state that waiting on background work is not a stall, with the Stop field described as observed, not documented.
- Checks: none (docs)
- Reviewers: —

## WP-10 Full check run
- Phase: 6 Polish
- Tasks: T020
- Files: any file of this feature, only to fix failures
- Depends on: WP-08, WP-09
- Parallel group: 6B
- Acceptance: all FRs — fmt, clippy and the whole workspace tests are green.
- Checks: cargo fmt --all, cargo clippy --workspace --all-targets -- -D warnings, cargo test --workspace
- Reviewers: —
