# Implementation Plan: End-to-end tests

**Branch**: `feature/LIBRE-8` | **Date**: 2026-10-22 | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `specs/005-e2e-tests/spec.md`

## Summary

A new crate, `crates/delilah-e2e`, holds a test harness. Each test starts a real
`delilahd` in a temp root, with its own `HOME`, `DELILAH_HOME`, free port, fixture
repos, fake `gh` and fake Jira server.

**Fake Claude.** The harness also points the daemon at a **fake Claude**: a Rust
binary that takes Claude Code's arguments and runs the hooks from the `--settings` file.
It follows a JSON scenario (trust dialog, ask, stop, input, fail, exit) and records what
it saw. A **tripwire** `claude` on the agents' PATH fails the test if the real one is
ever reached.

**Daemon changes.** It gets four env vars, all test-only:
- `DELILAH_AGENT_BIN`, into the existing `Config.agent_program`;
- `DELILAH_SECRETS=memory`;
- `DELILAH_JIRA_ALLOW_HTTP_LOOPBACK=1`;
- `DELILAH_PR_POLL_SECS`.

**The two layers.**
- **API/CLI:** 11 flows as Rust integration tests behind the crate's `e2e` feature.
- **UI:** 6 flows in Playwright (`app/e2e/`), against one daemon started by the
  harness's `e2e-env` binary and a Vite server pointed at it.

`scripts/e2e.sh` runs either layer or both. The real-Claude smoke test is opt-in. It
runs locally only, with no CI.

## Technical Context

**Language/Version**: Rust (edition 2024, workspace); TypeScript (`app/`)

**Primary Dependencies**:
- Existing: `reqwest` (blocking, as in the CLI), `serde_json`, `tempfile`, `axum` +
  `tokio` (fake Jira server only).
- New: `@playwright/test` (app devDependency, Chromium only).
- No new Rust crates beyond the workspace's.

**Storage**: none new; each Env has its own SQLite under its temp `DELILAH_HOME`.

**Testing**:
- `cargo test -p delilah-e2e`: unit tests of the fake's parsing and scenarios, run by
  `--workspace` too.
- `cargo test -p delilah-e2e --features e2e`: the flows.
- `cargo test -p delilah-daemon`: `Config::from_env` for the new vars.
- `cd app && npx playwright test`: UI flows.
- All through `scripts/e2e.sh`.

**Target Platform**: macOS (developer's Mac); local only, no CI.

**Project Type**: desktop app (daemon + CLI + web UI); this feature is test
infrastructure.

**Performance Goals**: API layer < 3 min, UI layer < 5 min after a build (SC-004). The
API tests run in parallel (one Env per test). UI tests run serially (R9).

**Constraints**:
- Never 7420, `~/.delilah`, the preview slots 7501–7509, or a stop by name.
- No real `claude`/`gh`/Jira and no Keychain.
- Agent env stays clean (the fake gets exactly what Claude would).
- No waits longer than 30 s, no fixed sleeps (FR-012).
- `cargo test --workspace` doesn't run the flows.

**Scale/Scope**:
- 1 new crate (lib, 2 bins, ~8 test files).
- 4 env vars in `config.rs`, wired in `secrets`/`jira`/`pr_loop`.
- `scripts/e2e.sh`.
- Playwright config and 6 specs.
- Docs: development, decisions, architecture, status.

## Constitution Check

`.specify/memory/constitution.md` is still the unfilled template; the project's ground
rules (`CLAUDE.md`) are the gates:

| Gate | Status |
|---|---|
| Nothing from the work repos; fixtures only | ✅ `make-fixture-parent.sh`, `jira-search.json`, fake `gh` data, all per Env |
| Agent hooks/settings outside the repo, clean env | ✅ the daemon still writes them under `DELILAH_HOME`; the fake reads them from `--settings`; `env.rs` untouched |
| Never restart/stop/`pkill` the installed daemon; never 7420 | ✅ free ports only, children killed by handle; `e2e.sh` unsets `DELILAH_URL/HOME/ADDR` |
| Try changes in a preview | ✅ the harness is a stricter preview; quickstart uses it |
| Confirm the account before remote operations | ✅ none (fake `gh`, tripwire `gh`) |
| Migrations never edited | ✅ no migration |
| Contracts `api.rs` ↔ `api.ts` | ✅ unchanged; new contracts are test-side ([contracts/](contracts/)) |
| Version bump per Spec Kit task | ✅ **patch** (0.4.0 → 0.4.1): tests and test-only daemon switches, no user-facing behaviour |

Re-checked after design: no violations.

## Project Structure

### Documentation (this feature)

```text
specs/005-e2e-tests/
├── spec.md
├── plan.md            # this file
├── research.md        # R1–R12
├── data-model.md      # Env, Scenario, Record, Fixtures
├── quickstart.md
├── contracts/
│   ├── daemon-env.md  # the four env vars
│   ├── fake-claude.md # args, hooks, terminal
│   └── e2e-env.md     # e2e-env binary and scripts/e2e.sh
└── checklists/
```

### Source Code (repository root)

```text
crates/delilah-daemon/src/
├── config.rs          # + agent_program from DELILAH_AGENT_BIN, secrets, jira_http_loopback, pr_poll
├── main.rs            # log the overrides
├── state.rs           # Secrets::Memory when config says so
├── jira.rs            # site check: http loopback when allowed
└── pr_loop/mod.rs     # tick from config instead of INTERVAL

crates/delilah-e2e/              # new, publish = false
├── Cargo.toml                   # feature "e2e"; [[test]] required-features
├── src/
│   ├── lib.rs                   # Env, EnvOptions, Api, Cli, wait_until
│   ├── env.rs                   # temp root, env -i, ports, start/stop, tripwire
│   ├── fixtures.rs              # fixture repos, fake gh data helpers
│   ├── jira_server.rs           # fake Jira (axum, background thread)
│   ├── fake/                    # shared by the fake-claude bin and tests
│   │   ├── args.rs              # Claude Code argv parsing
│   │   ├── scenario.rs          # Scenario, steps, selection
│   │   ├── hooks.rs             # run settings hooks with sh -c
│   │   └── record.rs            # Record
│   └── bin/
│       ├── fake-claude.rs
│       └── e2e-env.rs
└── tests/
    ├── fake_claude.rs           # US1-1…6
    ├── projects.rs              # US2-1, US2-11
    ├── tasks.rs                 # US2-2, 3, 4, 9
    ├── states.rs                # US2-5, US2-6
    ├── prs.rs                   # US2-7
    ├── jira.rs                  # US2-8
    ├── conversation.rs          # US2-10
    └── real_claude.rs           # US5, #[ignore] + DELILAH_E2E_REAL_CLAUDE=1

app/
├── package.json                 # + @playwright/test, "e2e" script
├── playwright.config.ts
└── e2e/
    ├── setup.ts                 # global setup/teardown: e2e-env start + Vite
    ├── fixtures.ts              # test fixture: api, fixture(), scenario()
    ├── projects.spec.ts         # US3-1
    ├── board.spec.ts            # US3-2
    ├── inbox.spec.ts            # US3-3
    ├── conversation.spec.ts     # US3-4
    ├── pr.spec.ts               # US3-5
    └── quick-open.spec.ts       # US3-6

scripts/e2e.sh
docs/development.md, docs/decisions.md, docs/architecture.md, docs/status.md
```

**Structure Decision**: one new workspace crate for everything Rust-side of the
harness (R7). The UI layer lives in `app/`, next to Vite, and reuses the harness
through the `e2e-env` binary instead of duplicating it in TypeScript. The vitest config
must exclude `app/e2e/` so `npx vitest run` doesn't pick the Playwright specs up.

## Phases (for tasks)

1. **Daemon switches**: the four env vars with unit tests in `config.rs` and their use
   (FR-001; contracts/daemon-env.md).
2. **Fake Claude**: argv parsing, scenarios, hook running, records, trust dialog,
   with unit tests. Then `tests/fake_claude.rs` against a real daemon (US1, FR-002…007).
3. **Harness**: `Env` (isolation, ports, tripwire, cleanup, log tail), `Api`/`Cli`
   helpers, `wait_until`, fixtures, fake gh helpers, fake Jira, `e2e-env` (FR-006a,
   008, 009, 012). In practice it lands together with phase 2's integration test.
4. **API/CLI flows**: the remaining test files (US2, FR-010, 011).
5. **UI layer**: Playwright setup and the 6 specs (US3, FR-013…015).
6. **Runner, smoke test, docs**: `scripts/e2e.sh`, `real_claude.rs`, the docs, a
   10-run stability check (SC-005), version bump (US4, US5, FR-016…018).

Phases 4 and 5 can run in parallel (no shared files) once phase 3 is done.

## Risks

- **The ask hook blocks the fake** until answered (curl `-m 86400`): the fake runs hooks
  in-process with the hook's own timeout, and tests always answer or release the ask.
  A forgotten ask shows up as a bounded-wait failure, not a hang of the suite, because
  the Env's drop kills the daemon, which closes the PTY.
- **PTY output of the trust dialog**: `trust_dialog_selection` keys on the `❯` marker and
  "yes,"/"no,". The fake prints exactly what it expects. If the real dialog changes, the
  smoke test is what notices.
- **Login-shell PATH under a temp HOME**: `/etc/zprofile`'s `path_helper` runs before
  `.zshrc`, so the tripwire dir is prepended in `.zshrc` (R3/R4). If `init_login_path`
  times out, agents get the daemon's PATH, which the harness also starts with the
  tripwire dir first.
- **Flakiness from timing**: every wait goes through `wait_until`, plus SC-005's 10-run
  check before the PR.

## Complexity Tracking

No gate violations.
