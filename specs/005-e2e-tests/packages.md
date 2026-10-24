# Work packages — 005-e2e-tests (End-to-end tests)

Source: specs/005-e2e-tests/tasks.md · 57 pending tasks · 19 packages

Check names: `rust:delilah-daemon` = `cargo test -p delilah-daemon`; `rust:delilah-e2e` = `cargo test -p delilah-e2e`; `rust:delilah-e2e+e2e` = `cargo test -p delilah-e2e --features e2e [--test X]`; `rust:workspace` = `cargo test --workspace`; `web` = `cd app && npx tsc --noEmit && npx vitest run`; `playwright` = `cd app && npx playwright test`.

## WP-01 Daemon test-only env switches
- Phase: 1 Foundational
- Tasks: T001, T002, T003, T004, T005, T006
- Files: crates/delilah-daemon/src/config.rs, crates/delilah-daemon/src/state.rs, crates/delilah-daemon/src/jira.rs, crates/delilah-daemon/src/pr_loop/mod.rs, crates/delilah-daemon/src/main.rs, crates/delilah-integrations/src/jira.rs (guess, only if `site_root` needs a fix), daemon test files with `Config { … }` literals (grep `agent_program: None`)
- Depends on: —
- Parallel group: 1A
- Acceptance: FR-001, FR-006, FR-008 — DELILAH_AGENT_BIN, DELILAH_SECRETS, DELILAH_JIRA_ALLOW_HTTP_LOOPBACK and DELILAH_PR_POLL_SECS are validated and wired; unset, the daemon behaves exactly as before
- Checks: rust:delilah-daemon, rust:delilah-integrations (only if touched)
- Reviewers: security

## WP-02 delilah-e2e crate, arg parsing and scenarios
- Phase: 2 Fake Claude
- Tasks: T007, T008, T009, T010, T011
- Files: crates/delilah-e2e/Cargo.toml, crates/delilah-e2e/src/lib.rs, crates/delilah-e2e/src/fake/mod.rs, crates/delilah-e2e/src/fake/args.rs, crates/delilah-e2e/src/fake/scenario.rs, crates/delilah-e2e/src/bin/fake-claude.rs
- Depends on: —
- Parallel group: 2A
- Acceptance: FR-002, FR-004 — new crate builds inside the workspace; argv parsing and scenario selection/parsing pass their unit tests
- Checks: rust:delilah-e2e, rust:workspace (build only)
- Reviewers: architecture

## WP-03 Fake Claude hooks and records
- Phase: 2 Fake Claude
- Tasks: T012, T013, T014, T015
- Files: crates/delilah-e2e/src/fake/hooks.rs, crates/delilah-e2e/src/fake/record.rs, crates/delilah-e2e/src/fake/mod.rs, crates/delilah-e2e/Cargo.toml (dev-dependency delilah-agents)
- Depends on: WP-02
- Parallel group: 2B
- Acceptance: FR-003, FR-005 — hooks from a real settings file run per event with timeouts; records are saved atomically and read back
- Checks: rust:delilah-e2e
- Reviewers: security

## WP-04 Fake Claude step runner and binary
- Phase: 2 Fake Claude
- Tasks: T016, T017, T018
- Files: crates/delilah-e2e/src/fake/run.rs, crates/delilah-e2e/src/bin/fake-claude.rs, crates/delilah-e2e/tests/fake_bin.rs, crates/delilah-e2e/src/fake/mod.rs
- Depends on: WP-02, WP-03
- Parallel group: 2C
- Acceptance: FR-002–FR-007 — the built fake-claude runs a scenario against a settings file, records it and exits with the scripted code, including the trust dialog
- Checks: rust:delilah-e2e
- Reviewers: —

## WP-05 Harness core: Env, wait_until, fixtures
- Phase: 3 Harness
- Tasks: T019, T020, T021, T022, T026
- Files: crates/delilah-e2e/src/env.rs, crates/delilah-e2e/src/lib.rs, crates/delilah-e2e/src/fixtures.rs, crates/delilah-e2e/Cargo.toml, scripts/make-fixture-parent.sh (read only)
- Depends on: WP-01, WP-04
- Parallel group: 3A
- Acceptance: FR-006a, FR-008, FR-009, FR-012 — Env builds a temp HOME layout with tripwires, starts and stops its own delilahd on a free port (never 7420/7501–7509), makes independent fixture repos and bounded waits
- Checks: rust:delilah-e2e, rust:delilah-e2e+e2e
- Reviewers: security

## WP-06 Harness clients: fake Jira, API, CLI
- Phase: 3 Harness
- Tasks: T023, T024, T025
- Files: crates/delilah-e2e/src/jira_server.rs, crates/delilah-e2e/src/api.rs, crates/delilah-e2e/src/cli.rs, crates/delilah-e2e/src/lib.rs, crates/delilah-e2e/Cargo.toml (dev-dependency delilah-integrations)
- Depends on: WP-05
- Parallel group: 3B
- Acceptance: FR-009, FR-011 — fake Jira answers the real client from the fixture; typed Api and Cli work against a real Env
- Checks: rust:delilah-e2e, rust:delilah-e2e+e2e
- Reviewers: —

## WP-07 e2e-env binary and flow test targets
- Phase: 3 Harness
- Tasks: T027, T028
- Files: crates/delilah-e2e/src/bin/e2e-env.rs, crates/delilah-e2e/tests/e2e_env_bin.rs, crates/delilah-e2e/Cargo.toml, stub files for each flow test (guess): crates/delilah-e2e/tests/{fake_claude,projects,tasks,states,prs,jira,conversation,real_claude}.rs
- Depends on: WP-05, WP-06
- Parallel group: 3C
- Acceptance: FR-013 (prerequisite) — e2e-env prints its JSON line, serves until stdin EOF, exits 3 on tripwire; flow tests need the `e2e` feature and don't build under `cargo test --workspace`
- Checks: rust:delilah-e2e, rust:delilah-e2e+e2e, rust:workspace
- Reviewers: —

## WP-08 US1 fake Claude end-to-end test
- Phase: 3 Harness
- Tasks: T029
- Files: crates/delilah-e2e/tests/fake_claude.rs, crates/delilah-e2e/src/env.rs (no_agent_bin knob)
- Depends on: WP-07
- Parallel group: 3D
- Acceptance: US1 acceptance 1–6, FR-001–FR-007 — a real daemon launches the fake, states, ask, resume, fail, trust dialog and tripwire all verified
- Checks: rust:delilah-e2e+e2e (--test fake_claude)
- Reviewers: —

## WP-09 Flows: projects and tasks
- Phase: 4 US2
- Tasks: T030, T031
- Files: crates/delilah-e2e/tests/projects.rs, crates/delilah-e2e/tests/tasks.rs, crates/delilah-e2e/src/env.rs (restart_daemon)
- Depends on: WP-08
- Parallel group: 4A
- Acceptance: FR-010, FR-011 — US2-1, 2, 3, 4, 9, 11: projects, worktree/submodule/checkout modes, delete, restart persistence, via API, CLI and disk
- Checks: rust:delilah-e2e+e2e (--test projects, --test tasks)
- Reviewers: —

## WP-10 Flows: states and PRs
- Phase: 4 US2
- Tasks: T032, T033
- Files: crates/delilah-e2e/tests/states.rs, crates/delilah-e2e/tests/prs.rs
- Depends on: WP-08
- Parallel group: 4A
- Acceptance: FR-010, FR-012 — US2-5, 6, 7: API/CLI/SSE agree on state and phase, ask answered, PR loop with failing checks, review and merge
- Checks: rust:delilah-e2e+e2e (--test states, --test prs)
- Reviewers: —

## WP-11 Flows: Jira and conversation
- Phase: 4 US2
- Tasks: T034, T035
- Files: crates/delilah-e2e/tests/jira.rs, crates/delilah-e2e/tests/conversation.rs
- Depends on: WP-08
- Parallel group: 4A
- Acceptance: FR-010 — US2-8, 10: task from fake Jira issue with in-memory secrets; task from idea via the confirmation card
- Checks: rust:delilah-e2e+e2e (--test jira, --test conversation)
- Reviewers: —

## WP-12 CLI coverage check
- Phase: 4 US2
- Tasks: T036
- Files: crates/delilah-e2e/tests/projects.rs, crates/delilah-e2e/tests/tasks.rs, crates/delilah-e2e/tests/states.rs (whichever lacks a CLI call)
- Depends on: WP-09, WP-10, WP-11
- Parallel group: 4B
- Acceptance: FR-011 — CLI project add, new, ls/show, done, archive and rm each exercised at least once; whole API layer green 3 runs in a row
- Checks: rust:delilah-e2e+e2e
- Reviewers: —

## WP-13 Playwright setup
- Phase: 5 US3
- Tasks: T037, T038, T039, T040, T041
- Files: app/package.json, app/package-lock.json, .gitignore, app/vite.config.ts, app/e2e/setup.ts, app/playwright.config.ts, app/e2e/fixtures.ts
- Depends on: WP-07
- Parallel group: 5A
- Acceptance: FR-013, FR-015 — Playwright runs headless Chromium against Vite plus an e2e-env daemon; vitest ignores app/e2e; failures attach trace, screenshot and daemon log
- Checks: web, playwright
- Reviewers: security

## WP-14 UI flows: projects, board, inbox
- Phase: 5 US3
- Tasks: T042, T043, T044
- Files: app/e2e/projects.spec.ts, app/e2e/board.spec.ts, app/e2e/inbox.spec.ts
- Depends on: WP-13
- Parallel group: 5B
- Acceptance: FR-014 — US3-1, 2, 3 pass in the browser
- Checks: web, playwright
- Reviewers: —

## WP-15 UI flows: conversation, PR, quick open
- Phase: 5 US3
- Tasks: T045, T046, T047
- Files: app/e2e/conversation.spec.ts, app/e2e/pr.spec.ts, app/e2e/quick-open.spec.ts
- Depends on: WP-13
- Parallel group: 5B
- Acceptance: FR-014 — US3-4, 5, 6 pass in the browser
- Checks: web, playwright
- Reviewers: —

## WP-16 Runner script and real-Claude smoke test
- Phase: 6 Runner/docs/release
- Tasks: T048, T050
- Files: scripts/e2e.sh, crates/delilah-e2e/tests/real_claude.rs, crates/delilah-e2e/src/env.rs (real_claude option, if not already present)
- Depends on: WP-08, WP-13
- Parallel group: 6A
- Acceptance: FR-016, FR-018 — one script runs api, ui or both with a summary and non-zero exit on failure; smoke test is ignored and opt-in
- Checks: rust:delilah-e2e+e2e, playwright
- Reviewers: security

## WP-17 Docs
- Phase: 6 Runner/docs/release
- Tasks: T051, T052, T053, T054
- Files: docs/development.md, docs/decisions.md, docs/architecture.md, docs/status.md
- Depends on: —
- Parallel group: 6A
- Acceptance: FR-017 — how to run and extend the tests, the decision, architecture and status are recorded
- Checks: —
- Reviewers: —

## WP-18 Verification and timing
- Phase: 6 Runner/docs/release
- Tasks: T049, T055, T056
- Files: specs/005-e2e-tests/progress.md, crates/delilah-e2e/src/env.rs (only if the Ctrl-C fix is needed), any flaky test file found
- Depends on: WP-12, WP-15, WP-16, WP-17
- Parallel group: 6B
- Acceptance: FR-008, FR-016, SC-004, SC-005 — installed daemon untouched, no leftovers after Ctrl-C, API < 3 min, UI < 5 min, 10 green runs, full workspace checks pass
- Checks: rust:workspace, cargo clippy --workspace --all-targets, tauri shell cargo test, web, scripts/e2e.sh
- Reviewers: —

## WP-19 Version bump
- Phase: 6 Runner/docs/release
- Tasks: T057
- Files: Cargo.toml, app/src-tauri/Cargo.toml, app/package.json, app/src-tauri/tauri.conf.json, Cargo.lock, app/src-tauri/Cargo.lock, app/package-lock.json
- Depends on: WP-18
- Parallel group: 6C
- Acceptance: patch bump to 0.4.1 (re-check main first) as its own commit `chore: bump version to 0.4.1`
- Checks: rust:workspace (cargo check)
- Reviewers: —
