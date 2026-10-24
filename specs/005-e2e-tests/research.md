# Research: End-to-end tests

Decisions taken while planning, with what was ruled out. They go to
`docs/decisions.md` when the feature lands (FR-017).

## R1. The fake Claude is a Rust binary, not a shell script

**Decision**: a new binary `fake-claude` in a new crate `crates/delilah-e2e`. It parses
the arguments the daemon passes (`claude::command`), reads the `--settings` JSON, and
runs the hook commands written there, exactly as Claude Code does: `sh -c <command>`
with the hook's JSON payload on stdin.

**Rationale**: the fake must parse JSON (settings, scenario, the ask hook's answer) and
read the PTY while it waits. A shell script would need `jq` or `python3`, neither of
which is guaranteed on the machine. Running the **settings' own hook commands** (rather
than posting to the daemon directly) is what makes the daemon unable to tell the fake
from the real Claude (FR-003): the curl commands, the query string with task and
session, the ask hook's timeout, are all exercised.

**Alternatives**: `scripts/fake-claude.sh` (the brief's idea), ruled out for the JSON
handling and for being hard to keep in step with `claude.rs`. A fake that posts to
`/hooks/claude` itself, ruled out: it would skip the settings file, which is half of
the contract.

## R2. The daemon learns the fake through `DELILAH_AGENT_BIN`

**Decision**: `Config::from_env` reads `DELILAH_AGENT_BIN` into the existing
`Config.agent_program` (today only unit tests set it). It must be an absolute path to
an executable file; otherwise the daemon refuses to start with a clear error (the edge
case "fake selected but missing": it fails at once instead of hanging). Unset, nothing
changes (FR-001).

**Rationale**: the field and its use in `run.rs` already exist; this only adds the env
var, like `DELILAH_GH_BIN`. Failing at start is cheaper to diagnose than a failing
session.

**Alternatives**: a CLI flag on `delilahd` (none exist today; env is the convention). A
`claude` shim first on `PATH`, ruled out: the agent's PATH comes from the login shell,
so it would be fragile, and it couldn't carry the scenario directory.

## R3. Each run's environment is fully isolated, including `HOME`

**Decision**: the harness starts `delilahd` with `env -i` (as `preview.sh` does) and:

| Variable | Value | Why |
|---|---|---|
| `HOME` | `<root>/home` | the daemon reads `~/.claude/settings.json` for the status line (`user_status_line`) and the login shell's rc files; neither may be the user's |
| `SHELL` | `/bin/zsh` | `env::init_login_path` runs `$SHELL -lic`; `<root>/home/.zshrc` puts the tripwire dir first on PATH |
| `PATH` | `<root>/tripwire:/usr/bin:/bin:/usr/sbin:/sbin` + git's dir | no brew, no user tools; tripwire first even if the login shell can't be read |
| `DELILAH_HOME` | `<root>/delilah` | its own state |
| `DELILAH_ADDR` | `127.0.0.1:<free port>` | never 7420, never 7501–7509 |
| `DELILAH_AGENT_BIN` | `<root>/bin/claude-fake` | wrapper that runs `fake-claude --dir <root>/fake` |
| `DELILAH_GH_BIN` | `<root>/bin/gh-fake` | wrapper that sets `FAKE_GH_DIR=<root>/gh` and runs `scripts/fake-gh.sh` |
| `DELILAH_SECRETS` | `memory` | the Jira token never reaches the user's Keychain |
| `DELILAH_JIRA_ALLOW_HTTP_LOOPBACK` | `1` | the fake Jira server is `http://127.0.0.1:<port>` |
| `DELILAH_ALLOW_FILE_PROTOCOL` | `1` | fixture submodules over `file://` |
| `DELILAH_NOTIFY` | `0` | no desktop notifications |
| `DELILAH_PR_POLL_SECS` | `1` | the PR loop's tick (R11) |
| `DELILAH_UI_ORIGIN` | the UI's URL (UI layer only) | CORS |

`<root>/home/.gitconfig` sets a test identity and `protocol.file.allow=always`.

**Rationale**: FR-006, FR-008 and FR-009 and the ground rules (clean env, nothing from
the user's machine). Wrappers carry per-run paths through the daemon's
`env_clear()`, which drops anything outside `KEPT_ENV`.

**Alternatives**: reuse `scripts/preview.sh`. Ruled out: it uses the user's `HOME`,
fixed slots and git-ignored folders in the repo, and it's an interactive tool. The
harness copies its approach (env -i, fixture script), not the script itself.

## R4. Tripwire for the real Claude

**Decision**: `<root>/tripwire/claude` and `<root>/tripwire/gh` are scripts that append
a line to `<root>/tripwire.log` and exit 1. The temp `.zshrc` puts `<root>/tripwire`
first on PATH, so it's also first on the agents' PATH. When an `Env` is dropped, the
harness fails the test if `tripwire.log` exists (FR-006a, SC-002). The smoke test
(R10) is the only one without the tripwire.

**Rationale**: the only way the real binary can be reached is through PATH (when
`DELILAH_AGENT_BIN` is unset or a code path ignores `agent_program`). The tripwire
catches exactly that, without depending on the user's install.

## R5. Free ports without a daemon change

**Decision**: the harness binds `127.0.0.1:0`, reads the port, drops the listener and
passes it as `DELILAH_ADDR`. If `delilahd` exits because the port was taken in between,
the harness retries with a new port, up to 3 times.

**Rationale**: the daemon builds hook URLs from `config.addr`, so letting it bind port
0 would need it to rewrite its config after binding. The race is tiny and the retry
closes it.

## R6. Fake Jira: a small HTTP server in the harness

**Decision**: the harness serves `crates/delilah-integrations/fixtures/jira-search.json`
(and per-issue JSON made from it) with a small axum server on a free loopback port in
a background thread. The test configures Jira through the API with that URL and any
token. The daemon accepts `http://` for a loopback site only when
`DELILAH_JIRA_ALLOW_HTTP_LOOPBACK=1`; `https://` stays the rule otherwise.

**Rationale**: the Jira client is real HTTP (reqwest); a server is the only fake that
tests it end to end. axum is already a workspace dependency.

**Alternatives**: a `DELILAH_JIRA_FIXTURE` file read by the daemon, ruled out because
it would bypass the HTTP client and the ADF parsing.

## R7. Where the API layer lives and how it stays out of `cargo test --workspace`

**Decision**: `crates/delilah-e2e` (workspace member, `publish = false`) with:
- `src/lib.rs`: the harness (`Env`, `Api`, `Cli`, `wait_until`, fake records, fixtures);
- `src/bin/fake-claude.rs`, `src/bin/e2e-env.rs`;
- `tests/*.rs`, each `[[test]]` with `required-features = ["e2e"]`.

`cargo test --workspace` builds the crate and its unit tests (fake Claude parsing) but
not the flows; `scripts/e2e.sh` runs `cargo test -p delilah-e2e --features e2e`.

**Rationale**: the workspace check stays fast (assumption in the spec); the harness is
shared with the UI layer through the `e2e-env` binary. Tests are plain sync Rust with
blocking `reqwest` (as the CLI uses) and `std::process`: no async in tests.

**Alternatives**: `crates/delilah-daemon/tests/`, ruled out: the harness drives several
binaries (daemon, CLI, fake) and would bloat the daemon's dev-dependencies. `#[ignore]`
on every flow, ruled out: it would mix them up with the real-Claude smoke test.

## R8. Choosing a fake scenario per task

**Decision**: scenarios are JSON files in `<root>/fake/scenarios/`. The fake picks one
by, in order: a `[fake:<name>]` marker in its prompt (tests put it in the task title or
notes, which reach the prompt); then `<task>.json` (task from the `?task=` in the
settings' hook URL); then `default.json`. The harness writes `default.json` as "start,
stop, wait for input" unless the test overrides it.

**Rationale**: the test controls the title before it knows the key; project sessions
and conversations (no title) are matched by task (`@<project>`) or the default.

## R9. UI layer: Playwright, one daemon and one Vite per run, serial tests

**Decision**: `@playwright/test` in `app/devDependencies`, `app/playwright.config.ts`,
tests in `app/e2e/*.spec.ts`. A global setup starts `target/debug/e2e-env` (which
prints a JSON line with the API URL, root and fixture paths, and lives until stdin
closes), then Vite on a free port with `VITE_DELILAH_URL` set to that API.
`workers: 1`; each test makes its own fixture copies (`e2e-env fixture --into`) and
registers them as its own projects, so tests share the daemon but not the repos.
Chromium only, headless; trace and screenshot retained on failure; the daemon log is
attached to the report.

**Rationale**: `VITE_DELILAH_URL` is fixed when Vite starts, so one daemon per test
would need one Vite per test (slow) or a runtime switch in the UI (code that exists
only for tests). Six flows serially fit SC-004.

**Alternatives**: a `?daemon=` query parameter in dev builds, ruled out for now (UI code
only for tests); Cypress, ruled out (Playwright is the common choice, has traces, runs
from Node without a separate app).

## R10. Real-Claude smoke test

**Decision**: one test in `crates/delilah-e2e/tests/real_claude.rs`, `#[ignore]` and
also requiring `DELILAH_E2E_REAL_CLAUDE=1`. It starts an `Env` without
`DELILAH_AGENT_BIN` and without the tripwire, with the user's real `HOME` for Claude's
login only through the clean agent env (as production), on a fixture repo with the
prompt "Reply OK and stop." It waits for `SessionStart` and `Stop` (via the task's
state) and closes the session. Run with `scripts/e2e.sh --real-claude`.

**Rationale**: FR-018; detects drift of the fake from Claude's real arguments, hooks
and folder-trust dialog.

**Note**: this test needs the user's `HOME` (Claude's login lives there), so it is
the one exception to R3's temp `HOME`; it still has its own `DELILAH_HOME` and port.

## R11. Bounded waits

**Decision**: one helper, `wait_until(what, timeout, || -> Option<T>)`, polls every
100 ms and on timeout panics with `what` and the last value seen (as `Debug`), plus the
tail of the daemon log. Default timeout 15 s, maximum 30 s (FR-012). PR polling: the PR
loop ticks every 30 s (`pr_loop::INTERVAL`), longer than the cap, so the daemon reads
`DELILAH_PR_POLL_SECS` (default 30, minimum 1) into `Config` and the harness sets it to 1. Playwright uses its own `expect` timeouts,
capped the same way.

## R12. Local only

**Decision** (clarification 2026-10-22): no CI workflow. `scripts/e2e.sh` is the entry
point; `docs/development.md` says to run it before a PR that touches the daemon, the
CLI or the UI.
