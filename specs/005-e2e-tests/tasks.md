# Tasks: End-to-end tests

**Input**: Design documents from `specs/005-e2e-tests/`
**Prerequisites**: plan.md, spec.md, research.md (R1–R12), data-model.md, contracts/ (daemon-env.md, fake-claude.md, e2e-env.md), quickstart.md

**Tests**: test-first. Write each test task before its implementation and watch it fail. For the flow files (Phases 4–5), the tests *are* the deliverable. "Watch it fail" there means: run it before the harness piece or the fixture helper it needs exists, or with a deliberately wrong expectation once, then fix the expectation.

**Checks**:
- Rust: `cargo test -p delilah-daemon`, `cargo test -p delilah-e2e`, flows `cargo test -p delilah-e2e --features e2e`.
- App: `cd app && npx tsc --noEmit && npx vitest run` (must not pick up `app/e2e/`), `npx playwright test`.
- At the end: `cargo test --workspace`, `cargo clippy --workspace --all-targets`, `scripts/e2e.sh`.

**Never** run anything against 7420 or `~/.delilah`, never `pkill`/`killall` a `delilahd`, and never launch the real `claude` (except T061, on purpose). Every test daemon is started by the harness and stopped by its own handle.

**Route lookup**: the exact HTTP methods and bodies for each flow are in `crates/delilah-daemon/src/api.rs` (router) and the client in `app/src/api.ts`. The CLI subcommands (`new`, `show`, `run`, `resume`, `done`, `archive`, `rm`, `project add`, …) are in `crates/delilah-cli/src/main.rs`. Read them; don't guess.

## Format: `[ID] [P?] [Story] Description`

---

## Phase 1: Daemon switches (plan phase 1; foundational)

**Purpose**: the four test-only env vars of contracts/daemon-env.md. They block everything else. Unset, nothing changes.

- [X] T001 Write failing unit tests in `crates/delilah-daemon/src/config.rs` (`#[cfg(test)]`, setting env vars through a helper that takes a closure over a `HashMap`, so tests don't race on the process env). If `from_env` reads `std::env` directly, first split it into `from_env()` → `from_vars(impl Fn(&str) -> Option<String>)`.
  Cases:
  - (a) `DELILAH_AGENT_BIN` unset → `agent_program == None`;
  - (b) set to an absolute executable (a temp file with mode 0o755) → `Some(path)`;
  - (c) relative path → error `DELILAH_AGENT_BIN must be an absolute path to an executable, got <value>`;
  - (d) missing file → same error;
  - (e) not executable (0o644) → same error;
  - (f) `DELILAH_SECRETS` unset → keychain; `memory` → memory; `other` → error `DELILAH_SECRETS must be "memory" or unset`;
  - (g) `DELILAH_JIRA_ALLOW_HTTP_LOOPBACK` unset → false, `1` → true;
  - (h) `DELILAH_PR_POLL_SECS` unset → 30 s; `1` → 1 s; `0`, `-3`, `abc` → error.
- [X] T002 Implement T001 in `crates/delilah-daemon/src/config.rs`:
  - New `Config` fields `secrets_in_memory: bool`, `jira_http_loopback: bool`, `pr_poll: Duration`; fill `agent_program` from `DELILAH_AGENT_BIN`.
  - Update every `Config { … }` literal in the daemon's tests (grep `agent_program: None`) with the defaults (false, false, 30 s), e.g. via a `Config::for_tests(home)` helper if one doesn't exist. Keep the doc comment on `agent_program`, updated to name the env var.
- [X] T003 Write a failing test, then make `crates/delilah-daemon/src/state.rs` pick `Secrets::Memory` when `config.secrets_in_memory` (keep `cfg!(test)` → memory as today).
- [X] T004 Write failing tests in `crates/delilah-daemon/src/jira.rs` (next to its existing tests) for the site check at the `starts_with("https://")` branch (~line 73):
  - `http://127.0.0.1:5000` and `http://localhost:5000` are rejected when `jira_http_loopback` is false and accepted when true;
  - `http://example.com` is rejected even when true.
  Then implement. Check `delilah_integrations::jira::site_root` keeps an `http://` root intact; fix it there with a unit test if not.
- [X] T005 Write a failing test, then make `crates/delilah-daemon/src/pr_loop/mod.rs` use `app.config.pr_poll` for the ticker instead of the `INTERVAL` const (keep the const as the default in config.rs; remove it here if unused). `NUDGE_DELAY` stays as is.
- [X] T006 In `crates/delilah-daemon/src/main.rs`, log at start (info) one line per override that is set: `agent program overridden` (with the path), `secrets in memory`, `jira over http on loopback allowed`, `pr poll every N s`. Config errors already exit non-zero; check the new ones do.

**Checkpoint**: `cargo test -p delilah-daemon` green; a daemon started without the new vars behaves exactly as before.

---

## Phase 2: Fake Claude (plan phase 2) — User Story 1 (P1), unit level

**Goal**: the `fake-claude` binary of contracts/fake-claude.md and data-model.md "Scenario"/"Record".

**Independent test**: `cargo test -p delilah-e2e` (unit tests). The end-to-end check of US1 against a real daemon is T029, once the harness exists.

- [X] T007 Create the crate `crates/delilah-e2e`:
  - `Cargo.toml`: `publish = false`, `version.workspace`, `edition.workspace`, `license.workspace`, `[lints] workspace = true`.
  - Dependencies: `serde`, `serde_json`, `reqwest` (`blocking`, `json`, no default features, as the CLI), `tempfile`, `axum`, `tokio` (rt-multi-thread, net, macros). Use the versions the other crates already use.
  - `[features] e2e = []`.
  - Empty `src/lib.rs`, `src/fake/mod.rs`, and `src/bin/fake-claude.rs` with `fn main() {}`.
  - Check `cargo test --workspace` still builds (the workspace uses `members = ["crates/*"]`).
- [X] T008 [P] [US1] Write failing unit tests in `crates/delilah-e2e/src/fake/args.rs` for argv parsing, using the exact arg vectors from the tests in `crates/delilah-agents/src/claude.rs` (`launch(...)` cases near lines 500–800):
  - new session;
  - `--resume`;
  - repeated `--add-dir`;
  - `--remote-control <name>`;
  - `--permission-mode dontAsk --setting-sources user --strict-mcp-config`;
  - `--plugin-dir`, `--append-system-prompt`;
  - `-- <prompt>`, including a prompt starting with `-`;
  - `--fake-dir <dir>` first;
  - an unknown option recorded, not an error;
  - errors for: missing `--settings`; both `--session-id` and `--resume`; neither.
- [X] T009 [US1] Implement `crates/delilah-e2e/src/fake/args.rs` (`FakeArgs { fake_dir, mode: New|Resume, session_id, settings, add_dirs, remote_control, permission_mode, setting_sources, strict_mcp, plugin_dir, append_system_prompt, prompt, unknown }`) until T008 passes.
- [X] T010 [P] [US1] Write failing unit tests in `crates/delilah-e2e/src/fake/scenario.rs`:
  - parsing every step of data-model.md (`trust_dialog`, `print`, `hook`, `sleep_ms` (capped at 5000), `ask`, `stop`, `wait_input`, `fail`, `exit`);
  - an unknown step is a parse error carrying the step;
  - selection order: `[fake:<name>]` in the prompt → `<task>.json` → `default.json`;
  - a marker naming a missing file is an error, not a silent default;
  - with no prompt and no task file it uses `default.json`.
- [X] T011 [US1] Implement `crates/delilah-e2e/src/fake/scenario.rs` (`Scenario { steps: Vec<Step> }`, `Step` enum with serde, `select(fake_dir, prompt, task) -> Result<(String, Scenario)>`) until T010 passes.
- [X] T012 [P] [US1] Write failing unit tests in `crates/delilah-e2e/src/fake/hooks.rs`:
  - Inputs: a settings JSON as produced by `delilah_agents::claude::settings(...)`. Add `delilah-agents` as a dev-dependency and call it with a fake URL. Replace the commands with `cat >> <tmp>/E.json` through a small helper that rewrites them.
  - Checks:
    - (a) `task` is extracted from the hook URL query (`?task=KEY&session=ID`);
    - (b) running event `E` runs every matching command with the payload on stdin and the hook's `timeout`;
    - (c) `PreToolUse` runs only the `AskUserQuestion` matcher and returns its stdout;
    - (d) a hook that exits non-zero is recorded with its code, not an error;
    - (e) a hook past its timeout is killed and recorded as code `-1`;
    - (f) payload shapes per event match the table in contracts/fake-claude.md.
- [X] T013 [US1] Implement `crates/delilah-e2e/src/fake/hooks.rs` (`Hooks::load(settings_path)`, `task()`, `run(event, payload) -> HookRun { exit_code, stdout }`, payload builders) until T012 passes. Use `std::process::Command::new("sh").arg("-c")`, `cwd` = the process's cwd.
- [X] T014 [P] [US1] Write failing unit tests in `crates/delilah-e2e/src/fake/record.rs`:
  - `Record` serialises with every field of data-model.md "Record";
  - `save()` writes atomically (temp file + rename) to `<fake_dir>/records/<session>.json`;
  - `load(fake_dir, session)` and `for_task(fake_dir, task)` (latest by mtime) read it back.
- [X] T015 [US1] Implement `crates/delilah-e2e/src/fake/record.rs` until T014 passes.
- [X] T016 [US1] Write failing unit tests for the step runner in `crates/delilah-e2e/src/fake/run.rs`. It is generic over `R: BufRead` (terminal input) and `W: Write` (terminal output) so tests drive it with in-memory pipes and a settings file whose hooks write to temp files.
  Cases:
  - `SessionStart` always first, with `source` `startup` or `resume`;
  - `print` writes the text;
  - `trust_dialog` writes exactly the text of contracts/fake-claude.md, and `claude::trust_dialog_selection(output)` on it returns `Some(true)` (dev-dependency on `delilah-agents`; `trust_dialog_selection` is `pub`); Enter goes on, `2\n` exits 1;
  - `ask`:
    - runs `PreToolUse` with `tool_input.questions`;
    - a stdout of `{"hookSpecificOutput":{"updatedInput":{"answers":{"q":"A"}}}}` is recorded in `answers` and followed by `PostToolUse`;
    - empty stdout records `null` and then reads one line from input as the answer;
  - `stop` runs `Stop` with `last_assistant_message`;
  - `wait_input` records the line and runs `UserPromptSubmit` with `prompt`;
  - `fail` runs `StopFailure` and exits 1;
  - `exit` runs `SessionEnd` and exits with its code;
  - after the last step: each typed line → recorded, `UserPromptSubmit` + `Stop`;
  - EOF → `SessionEnd` and exit 0;
  - the record is saved after each step.
- [X] T017 [US1] Implement `crates/delilah-e2e/src/fake/run.rs` until T016 passes.
- [X] T018 [US1] Implement `crates/delilah-e2e/src/bin/fake-claude.rs`:
  - Steps: parse args → load hooks → select scenario → record `args`, `env` (all of `std::env::vars()`), `cwd`, `scenario` → run on stdin/stdout.
  - SIGHUP: run `SessionEnd` and exit 0. Use the `signal-hook`-free approach: treat a read error or EOF on stdin as hang-up, since the PTY closing gives EOF.
  - Errors before the first step: print to stderr and exit 2. Errors with hooks already loaded run `StopFailure` first.
  - Add a test in `crates/delilah-e2e/tests/fake_bin.rs` (not behind the `e2e` feature). It runs the built binary (`env!("CARGO_BIN_EXE_fake-claude")`) with a temp settings file and the scenario `[print, stop, exit 0]` and checks the record and the exit code.

**Checkpoint**: `cargo test -p delilah-e2e` green; the fake runs by hand with a settings file from a preview.

---

## Phase 3: Harness (plan phase 3; foundational for US2 and US3) + US1 end to end

**Purpose**: `Env` and helpers of data-model.md, the fake Jira, `e2e-env`, and US1's acceptance against a real daemon.

- [X] T019 Write failing tests in `crates/delilah-e2e/src/env.rs` (unit, no daemon):
  - `free_port()` never returns 7420 or 7501–7509 (inject a fake picker that returns 7420 first);
  - `layout(root)` creates `home/` (with `.zshrc` prepending `<root>/tripwire` to PATH, `.gitconfig` with `user.name=Delilah E2E`, `user.email=e2e@delilah.invalid`, `protocol.file.allow=always`, an empty `.claude/`), `delilah/`, `bin/claude-fake` and `bin/gh-fake` (executable wrappers with absolute paths, contents as in contracts/fake-claude.md and research R3), `tripwire/claude` and `tripwire/gh` (append `"<name> $*"` to `<root>/tripwire.log`, exit 1), `fake/scenarios/default.json` (`[{"stop": "Fake Claude stopped."}]`), `fake/records/`, and `gh/` with `repo.json` and `user.json`;
  - `daemon_env(root, port, opts)` returns exactly the variables of research R3's table and nothing else (in particular no `DELILAH_URL`, no `CLAUDE*`, no `ANTHROPIC*`, and `HOME` = `<root>/home`).
- [X] T020 Implement `crates/delilah-e2e/src/env.rs` until T019 passes:
  - `Env::start(EnvOptions)`:
    - build the layout;
    - make fixtures (T022);
    - start `delilahd` (path: `target/debug/delilahd`, found from `CARGO_MANIFEST_DIR/../../target/debug`, overridable with `DELILAH_E2E_TARGET`) with `env_clear()` + `daemon_env`, stdout/stderr to `<root>/delilahd.log`;
    - poll `GET /api/v1/health` with `wait_until` (15 s);
    - if the child exits early and the log says the address is in use, retry with a new port (max 3).
  - `Drop`:
    - `child.kill()` + `wait()`, never anything by name;
    - stop the Jira thread;
    - if `tripwire.log` exists and not `real_claude`, panic with its contents ("the real claude/gh was run"), but not while already panicking: print it instead;
    - keep `root` when `DELILAH_E2E_KEEP=1` and the thread is panicking, printing its path.
  - `EnvOptions` as in data-model.md.
- [X] T021 [P] Write failing tests, then implement `wait_until(what, timeout, f)` in `crates/delilah-e2e/src/lib.rs`:
  - polls every 100 ms;
  - default 15 s; any timeout above 30 s is clamped to 30 s;
  - on timeout it panics with `what`, the last `Debug` value seen and, through an optional `&Env`, the last 40 lines of the daemon log.
  Also `Env::wait_until` as the method form.
- [X] T022 [P] Write failing tests, then implement `crates/delilah-e2e/src/fixtures.rs`:
  - `Fixtures::make(into)` runs `scripts/make-fixture-parent.sh <into>/parent-env` (path from `CARGO_MANIFEST_DIR`) and `git -c protocol.file.allow=always clone -q <into>/parent-env/remotes/svc-orders.git <into>/signal`, as `scripts/preview.sh` lines ~45–46 do, with `HOME` set to the Env's home so the user's git config isn't read;
  - fields `repo` (`signal`), `parent` (`parent-env/parent`), `remotes`;
  - fake `gh` helpers that write the JSON shapes `scripts/fake-gh.sh` reads: `open_pr(n, branch)`, `set_checks(n, failing|passing)`, `add_review(n, body)`, `merge(n)`. Mirror the JSON in `specs/003-pr-loop/quickstart.md` and `crates/delilah-daemon/src/pr_testkit.rs`.
  - Test: two `make` calls give independent repos (a branch made in one doesn't exist in the other).
- [X] T023 [P] Write failing tests, then implement `crates/delilah-e2e/src/jira_server.rs`:
  - an axum server on `127.0.0.1:0` in a background thread with its own tokio runtime, with a shutdown handle;
  - routes `/rest/api/3/myself`, `/rest/api/3/field`, `/rest/api/3/search/jql`, `/rest/api/3/issue/{key}`, answered from `crates/delilah-integrations/fixtures/jira-search.json` (data-model.md "Fixtures");
  - the test calls `delilah_integrations::jira` (dev-dependency) against it and gets the fixture's issues, so the shapes match the real client.
- [X] T024 [P] Write failing tests, then implement the typed API client `Api` in `crates/delilah-e2e/src/api.rs`:
  - blocking reqwest, base URL from `Env`;
  - one method per route the flows use, with request/response types taken from `crates/delilah-daemon/src/api.rs` and `app/src/api.ts`, deserialised as `serde_json::Value` plus small typed accessors where tests assert. Methods: `health`, `projects`, `add_project` (whatever the UI's "add project" calls, see `app/src/api.ts`), `inspect_project`, `new_task`, `start_run`, `tasks`, `task`, `set_phase`, `done`, `archive`, `delete_task`, `resume`, `inbox`, `answer_ask`, `release_ask`, `sessions`, `reply`, `prs`, `jira_configure`, `jira_issues`, `events` (SSE reader collecting topics for N ms, bounded).
  - Test against a real `Env`: `health` OK. Put this test behind `#[cfg(feature = "e2e")]`.
- [X] T025 [P] Write failing tests, then implement `Cli` in `crates/delilah-e2e/src/cli.rs`: runs `target/debug/delilah` with `env_clear()`, `HOME` = Env home, `PATH` = `/usr/bin:/bin`, `DELILAH_URL` = Env api. Returns `(status, stdout, stderr)`. Test: `delilah ls` on an empty Env exits 0 (behind `e2e`).
- [X] T026 Add `Env` helpers in `crates/delilah-e2e/src/env.rs`:
  - `scenario(name, json)` writes `fake/scenarios/<name>.json`;
  - `record(session)`, `record_for(task)` (via `fake::record`);
  - `register_fixtures()` adds `repo` as a repo project and `parent` as a parent project and returns their names;
  - `log_tail(n)`.
- [X] T027 Implement `crates/delilah-e2e/src/bin/e2e-env.rs` per contracts/e2e-env.md:
  - `start [--ui-origin <url>] [--jira] [--keep]`: prints one JSON line, then waits until stdin hits EOF or SIGINT/SIGTERM. Use `ctrlc`-free handling: a thread reading stdin, plus a `tokio::signal` in the Jira runtime if needed. Then it drops the Env and exits 3 if the tripwire fired;
  - `fixture --into <dir>`.
  - Test in `crates/delilah-e2e/tests/e2e_env_bin.rs` (behind `e2e`): spawn it, read the line, `GET <api>/api/v1/health` OK, close stdin, it exits 0 and the daemon's port is closed.
- [X] T028 Add `[[test]]` entries with `required-features = ["e2e"]` to `crates/delilah-e2e/Cargo.toml` for every flow file (`fake_claude`, `projects`, `tasks`, `states`, `prs`, `jira`, `conversation`, `e2e_env_bin`, `real_claude`); `fake_bin` stays without it. Check `cargo test --workspace` doesn't build or run them.
- [ ] T029 [US1] Write `crates/delilah-e2e/tests/fake_claude.rs` covering US1 acceptance 1–6 against a real `Env`:
  1. start a task (`[fake:echo]`) and check the record: `cwd` = the worktree, `mode` new, `session_id` = the daemon's session id for the task, `settings` exists and holds the hooks, `prompt` contains the title, `env` keys ⊆ `KEPT_ENV` + `TERM`, `COLORTERM`;
  2. default scenario → the task's live state goes working → waiting, read through the API;
  3. `ask` scenario → the ask appears in `inbox`; `answer_ask` → the record's `answers[0]` holds it;
  4. after stop, `resume` → a new record with `mode` resume and the same `session_id`;
  5. `fail` scenario → the session is shown as failed;
  6. `Env` with `DELILAH_AGENT_BIN` removed (an `EnvOptions` test-only knob `no_agent_bin`) → starting a task hits the tripwire. Assert that `tripwire.log` contains `claude`, then delete it so the drop doesn't fail.
  Plus: a `trust_dialog` scenario whose task still reaches working, which proves `accept_trust_dialog` answered it (check the record's `inputs`/step count).

**Checkpoint**: US1 fully done; `cargo test -p delilah-e2e --features e2e --test fake_claude` green with the installed Delilah running and untouched.

---

## Phase 4: User Story 2 — API/CLI flows (P1) ⟂ parallel with Phase 5

**Goal**: the 11 flows of spec US2 (FR-010, FR-011, FR-012). Each test owns one `Env`; tests run in parallel.

**Independent test**: `cargo test -p delilah-e2e --features e2e` green in < 3 min after a build; no process left running.

- [ ] T030 [P] [US2] `crates/delilah-e2e/tests/projects.rs`:
  - US2-1: register `repo` and `parent` through the CLI `project add` and check `projects` lists both with kinds `repo` and `parent`;
  - US2-11: create and start a task, drop only the daemon child (an `Env::restart_daemon()` helper that kills the child by handle and starts it again on the same home and port), then check the projects, the task and its session are still listed.
- [ ] T031 [P] [US2] `crates/delilah-e2e/tests/tasks.rs`, flows US2-2, US2-3, US2-4 and US2-9:
  - US2-2: new task in worktree mode on `repo`, start it:
    - `git worktree list` in the fixture shows `<DELILAH_HOME>/workspaces/<KEY>` on `feature/<KEY>`;
    - the task is working;
    - `delilah ls` (CLI) lists the key.
  - US2-3: on `parent`, choose one of the fixture's submodules:
    - only that submodule is initialised in the worktree (`git submodule status` shows `-` for the others);
    - all on `feature/<KEY>`.
  - US2-4: checkout mode on `repo`:
    - while running, the fixture's checkout is on `feature/<KEY>`;
    - after `archive`, it is back on the branch it was on before.
  - US2-9: a backlog task is deleted (API, and once via `delilah rm`), then `tasks` no longer has it and `<DELILAH_HOME>/tasks/<KEY>` is gone.
- [ ] T032 [P] [US2] `crates/delilah-e2e/tests/states.rs`, flows US2-5 and US2-6:
  - US2-5:
    - default scenario: wait until the state is waiting;
    - `set_phase` to the next phase of the task's workflow (read the valid ones from `crates/delilah-daemon/src/phases.rs`);
    - mark done via `delilah done`;
    - assert that API `task`, `delilah show` and the SSE `events` all report the same state and phase at each step.
  - US2-6:
    - `ask` scenario: the ask shows in `inbox`;
    - `answer_ask`: the ask leaves the inbox, the record has the answer, and the task goes back to working and then waiting (the scenario's `stop` after the ask).
- [ ] T033 [P] [US2] `crates/delilah-e2e/tests/prs.rs`, flow US2-7, with `Env` `pr_poll` 1 s and the fake `gh` helpers:
  - start a task;
  - `open_pr(1, feature/<KEY>)`: wait until `prs` shows PR 1 for the task;
  - `set_checks(1, failing)`: the task shows failing checks;
  - `add_review(1, …)`: the review is visible;
  - `merge(1)`: the task is archived and its local branch is deleted (as `archive.rs` does);
  - every wait ≤ 30 s.
- [ ] T034 [P] [US2] `crates/delilah-e2e/tests/jira.rs`, flow US2-8, with `EnvOptions { jira: true }`:
  - configure Jira through the API with the fake server URL, any email and token;
  - `jira_issues` lists `PROJ-1242` and the other fixture issues;
  - create a task from `PROJ-1242` the way the UI's Jira view does (see `app/src/components/JiraView.tsx` and `app/src/api.ts`);
  - the task has key `PROJ-1242` and the fixture's summary as title;
  - nothing was written to the Keychain: `secrets in memory` is in the log.
- [ ] T035 [P] [US2] `crates/delilah-e2e/tests/conversation.rs`, flow US2-10:
  - open a project conversation ("task from an idea") through the API, as `app/src/components/ConversationView.tsx` / `NewTaskDialog.tsx` do;
  - its scenario `@<project>.json` is selected by task: an `ask` whose shape is the confirmation card of `specs/004-task-from-idea/contracts/api.md` (`header: "Nueva tarea"`, title as question, brief as the `Crear` option's `preview`);
  - answer `Crear`: a new backlog task exists with that title, and its prompt/brief is the preview text;
  - the record's `answers[0]` carries the new key, as decision 004 says.
- [ ] T036 [US2] Make sure FR-011 holds: at least one CLI call each for project add, new, ls/show, done, archive and rm across T030–T035. Add the missing ones to the matching file.

**Checkpoint**: US2 done; 11 flows green; run the suite 3 times in a row with no failure.

---

## Phase 5: User Story 3 — UI flows in a browser (P2) ⟂ parallel with Phase 4

**Goal**: the 6 flows of spec US3 (FR-013–FR-015). Needs Phase 3 (the `e2e-env` binary), not Phase 4.

**Independent test**: `cd app && npx playwright test` green headless in < 5 min.

- [X] T037 [US3] Add `@playwright/test` to `app/package.json` devDependencies (pin the version like the other deps), plus a script `"e2e": "playwright test"`, then `npm install`. Add `app/playwright-report/` and `app/test-results/` to `.gitignore`.
- [X] T038 [US3] Exclude `e2e/**` from vitest in `app/vite.config.ts`: `test: { exclude: [...configDefaults.exclude, "e2e/**"] }` with `configDefaults` from `vitest/config`, keeping whatever test config exists. Check `npx vitest run` still runs the same 15+ files and no `*.spec.ts` from `e2e/`.
- [X] T039 [US3] Write `app/e2e/setup.ts` (Playwright `globalSetup`/`globalTeardown`):
  - pick a free port P for Vite;
  - spawn `../target/debug/e2e-env start --jira --ui-origin http://localhost:P` (fail with a clear message if the binary is missing: "run cargo build -p delilah-e2e");
  - read its JSON line;
  - spawn `npx vite --port P --strictPort` with `VITE_DELILAH_URL` = `api`;
  - wait for both; write the JSON plus the UI URL to `app/test-results/e2e-env.json` for the tests;
  - teardown: kill Vite by its handle, close e2e-env's stdin and await it, then fail if it exited 3 (tripwire).
  Neither process is ever killed by name.
- [X] T040 [US3] Write `app/playwright.config.ts`:
  - `testDir: "e2e"`, `workers: 1`, `fullyParallel: false`;
  - Chromium only, headless;
  - `use.baseURL` from the env file;
  - `trace: "retain-on-failure"`, `screenshot: "only-on-failure"`;
  - `expect.timeout` 15 s, test timeout 60 s;
  - global setup and teardown from T039.
- [X] T041 [US3] Write `app/e2e/fixtures.ts`: a Playwright `test.extend` with:
  - `api` (fetch to the daemon);
  - `fixture()`, which runs `e2e-env fixture --into <tmp>` and returns paths;
  - `scenario(name, json)`, which writes into `fake_dir/scenarios`;
  - `project(kind)`, which makes a fixture and registers it under a unique name.
  On failure it attaches the daemon log tail (from `log`) to the test report (FR-015).
- [X] T042 [P] [US3] `app/e2e/projects.spec.ts` (US3-1): add the fixture repo as a project through the UI's add-project flow (find it from `app/src/App.tsx`/the sidebar); it appears in the project list.
- [X] T043 [P] [US3] `app/e2e/board.spec.ts` (US3-2): in `NewTaskDialog`, create and start a task titled `… [fake:slow-stop]`. The scenario is `[{"sleep_ms": 1500}, {"stop": "ok"}]`. The card appears on `BoardView` in the working column, then moves to the waiting column.
- [X] T044 [P] [US3] `app/e2e/inbox.spec.ts` (US3-3): `ask` scenario. Open `InboxView`; the `AskCard` shows the question; choose an option. The card disappears and the task shows working again.
- [ ] T045 [P] [US3] `app/e2e/conversation.spec.ts` (US3-4): start "Nueva tarea con Claude" on a project. The confirmation-card scenario (as T035) shows the card in `ConversationView`; click `Crear`. The new task opens.
- [ ] T046 [P] [US3] `app/e2e/pr.spec.ts` (US3-5): start a task and write a fake PR with failing checks and one review comment (via `gh_dir`, mirroring T022's helpers in TS). Open the task's `PrView`: the PR state, the failing check and the comment are shown.
- [ ] T047 [P] [US3] `app/e2e/quick-open.spec.ts` (US3-6): create three backlog tasks with distinct titles. Open quick open with its in-app shortcut (see `app/src/shortcuts.ts`; the in-page one, not the native global ⌘P), type part of one title; it's the first result, and Enter shows that task.

**Checkpoint**: US3 done; `npx playwright test` green 3 times in a row; a deliberately broken assertion leaves a trace, a screenshot and the daemon log in the report.

---

## Phase 6: Runner, smoke test, docs, release (plan phase 6) — User Stories 4 (P2) and 5 (P3)

- [ ] T048 [US4] Write `scripts/e2e.sh` per contracts/e2e-env.md:
  - `set -euo pipefail`;
  - `unset DELILAH_URL DELILAH_HOME DELILAH_ADDR DELILAH_AGENT_BIN DELILAH_GH_BIN FAKE_GH_DIR` first;
  - args `api|ui|--real-claude`, `-- <filter>`;
  - build `cargo build -p delilah-daemon -p delilah-cli -p delilah-e2e`;
  - API: `cargo test -p delilah-e2e --features e2e -- <filter>`;
  - UI: `npm --prefix app ci` if `app/node_modules` is missing, `npx --prefix app playwright install chromium` (idempotent), `npm --prefix app run e2e`;
  - summary lines `api: PASS|FAIL (…s)` and `ui: PASS|FAIL (…s)`; exit non-zero if any failed;
  - `--help` from the header comment, like `scripts/preview.sh`.
- [ ] T049 [US4] Check US4-3 and US4-4 by hand and note the results in `specs/005-e2e-tests/progress.md`:
  - with the installed Delilah running, note its daemon PID (`lsof -nP -iTCP:7420 -sTCP:LISTEN`);
  - run `scripts/e2e.sh`, then confirm the PID is the same and `curl -s 127.0.0.1:7420/api/v1/health` is OK;
  - interrupt a run with Ctrl-C mid-API-layer, then confirm `pgrep -fl 'e2e-env|fake-claude'` lists nothing and no `delilahd` other than the installed one is listening (`lsof -nP -iTCP -sTCP:LISTEN | grep delilahd`).
  If Ctrl-C leaves daemons behind (the test binary is killed before `Drop`), fix it: start each `delilahd` in its own process group and add a `ctrlc`-style handler in the harness that kills the recorded children by PID. Never kill by name.
- [ ] T050 [US5] Write `crates/delilah-e2e/tests/real_claude.rs`:
  - one `#[ignore]` test that also returns early unless `DELILAH_E2E_REAL_CLAUDE=1`;
  - `EnvOptions { real_claude: true }`: no `DELILAH_AGENT_BIN`, no tripwire, `HOME` = the user's real HOME (research R10), own `DELILAH_HOME` and port;
  - a task on `repo` titled `Reply OK and stop.`;
  - wait (≤ 30 s each) for working, then waiting; close the session through the API and archive the task.
  Add `--real-claude` to `scripts/e2e.sh` running `cargo test -p delilah-e2e --features e2e --test real_claude -- --ignored` with the env var set and a warning line that it uses the subscription.
- [ ] T051 [P] [US4] Update `docs/development.md` with a new section "End-to-end tests" after "Checks":
  - what the two layers are;
  - `scripts/e2e.sh` usage;
  - that it's safe with the installed Delilah running (temp HOME, free ports, tripwire);
  - run it before a PR that touches the daemon, the CLI or the UI (local only, no CI);
  - how to add an API flow and a UI flow (from quickstart.md "Adding a flow");
  - the scenario step table (from data-model.md);
  - `DELILAH_E2E_KEEP=1`;
  - the smoke test and its cost.
  Also add the new env vars to wherever the doc lists the daemon's env vars, marked test-only.
- [ ] T052 [P] [US4] Add a decision to `docs/decisions.md` (new numbered entry under "Working on Delilah"): end-to-end tests with a fake Claude. Cover R1 (Rust binary that runs the settings' hooks), R2 (`DELILAH_AGENT_BIN`), R3/R4 (temp HOME and tripwire), R7 (separate crate behind a feature), R9 (one daemon per UI run, serial), R12 (local only), and what was ruled out (shell fake, replaying real sessions, CI on macOS runners, a UI runtime daemon switch).
- [ ] T053 [P] [US4] Update `docs/architecture.md`:
  - crates table: add `crates/delilah-e2e` (harness, `fake-claude`, `e2e-env`);
  - `scripts/` row: add `e2e.sh`;
  - mention `app/e2e/`;
  - in the daemon config, the four test-only env vars.
- [ ] T054 [P] [US4] Update `docs/status.md`: E2E tests built (17 flows, fake Claude, local only), plus anything left pending or found flaky.
- [ ] T055 Run the full checks: `cargo test --workspace`, `cargo clippy --workspace --all-targets` (no new warnings), `cargo test --manifest-path app/src-tauri/Cargo.toml` (after `scripts/build-sidecar.sh debug` if needed), `cd app && npx tsc --noEmit && npx vitest run`, `scripts/e2e.sh`.
- [ ] T056 SC-004 / SC-005: time `scripts/e2e.sh api` and `scripts/e2e.sh ui` (API < 3 min, UI < 5 min after a build), then run `scripts/e2e.sh` 10 times in a row. Record times and results in `specs/005-e2e-tests/progress.md`. Fix any flaky test by replacing its timing assumption with a `wait_until`, never by raising a timeout above 30 s.
- [ ] T057 Version bump, patch 0.4.0 → 0.4.1 (re-check `main`'s version first with `git show main:Cargo.toml | grep '^version'`):
  - edit the four places of `docs/development.md#releasing`: `Cargo.toml` (workspace), `app/src-tauri/Cargo.toml`, `app/package.json`, `app/src-tauri/tauri.conf.json`;
  - refresh the lockfiles: `cargo check --workspace`, `(cd app/src-tauri && cargo check)`, `(cd app && npm install --package-lock-only)`;
  - commit as its own commit `chore: bump version to 0.4.1`.
  The PR says "patch: tests and test-only daemon switches, no user-facing behaviour".

---

## Dependencies & execution order

```text
Phase 1 (daemon switches) ──► Phase 2 (fake Claude, unit) ──► Phase 3 (harness + US1 e2e)
                                                                   │
                                              ┌────────────────────┴───────────────────┐
                                              ▼                                        ▼
                                  Phase 4 (US2 API/CLI flows)        ⟂        Phase 5 (US3 UI flows)
                                              └────────────────────┬───────────────────┘
                                                                   ▼
                                                    Phase 6 (US4 runner/docs, US5 smoke, release)
```

- Phase 2 needs T007 (crate) but not Phase 1; T029 needs Phase 1 (T002) and Phase 3.
- **Phases 4 and 5 run in parallel**: Phase 4 touches only `crates/delilah-e2e/tests/*`, Phase 5 only `app/` and `.gitignore`. T046 mirrors T022's gh JSON in TS rather than sharing code.
- Inside Phase 4, T030–T035 are independent files ([P]); T036 last.
- Inside Phase 5, T037–T041 in order, then T042–T047 in parallel.
- T048 before T049, T050 and T056; the docs tasks T051–T054 are [P]; T057 last.

## Parallel examples

```text
Phase 2: T008, T010, T012, T014 (tests in different files), then their implementations.
Phase 3: T021, T022, T023, T024, T025 after T020.
Phase 4 + Phase 5 together: one worker on T030–T036, another on T037–T047.
Phase 6: T051, T052, T053, T054.
```

## Implementation strategy

- **MVP**: Phases 1–3. The fake Claude, the harness and US1's test give a safe, isolated daemon with a scriptable agent, which every other test builds on.
- Then US2 (the cheapest, most valuable coverage), with US3 alongside if there are two workers.
- Then the runner, docs and release. US5 (smoke) is small and last; it costs one real session to check.
