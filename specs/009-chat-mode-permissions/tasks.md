---
description: "Tasks: chat mode asks for permission as often as terminal mode"
---

# Tasks: Chat mode asks for permission as often as terminal mode

**Input**: Design documents from `specs/009-chat-mode-permissions/`

**Prerequisites**:
- plan.md, spec.md;
- research.md: R1 is the planning probe;
- data-model.md;
- contracts/agent-permissions.md;
- quickstart.md.

**Tests**: these are required. The spec (US3, FR-002, FR-007) and the user ask for
test-first work. Each test task is written before its implementation and must FAIL on
the current code. Record the failing run in `progress.md` before implementing.

**Ground rules**:
- Never restart, stop or `pkill` the installed daemon (7420), and never use
  `scripts/dev-daemon.sh`. Try changes only with `scripts/preview.sh`.
- The spike runs the real `claude` in throwaway folders, never in the repo or `~/.delilah`.
- No version bump.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: can run in parallel (different files, no dependency on an unfinished task)
- **[Story]**: US1 (autopilot unattended in chat), US2 (hands-on chat without needless
  cards), US3 (proven before changed)

---

## Phase 1: Setup, the spike (gate for everything else, FR-001)

**Purpose**: confirm, against the real CLI, the behaviour every later task relies on.

- [X] T001 Extend `crates/delilah-agents/examples/claude_stream_spike.rs`:
  - add a `--permission-mode <mode>` option, passed through to every `claude` it starts,
    PTY and stream;
  - add a `routine` scenario with `--permission-prompt-tool stdio` and a fresh
    `git init` workspace containing an executable `run.sh`. It asks the model to:
    1. write `notes.txt`;
    2. `sed -i "" "s/a/b/" notes.txt`;
    3. run `./run.sh`;
    4. `git add -A && git commit`;
    5. try a list of candidate "control" actions that auto mode might escalate (for
       example `rm -rf ~/.ssh-spike-canary`, which targets a non-existent canary path,
       `git push --force origin main` with no remote, and writing outside the
       workspace to `$HOME/.delilah-spike-canary`). Every candidate must be harmless
       even if allowed.
  - answer each `can_use_tool` with deny;
  - write to `summary.txt`, per action: asked (`can_use_tool`), refused without an ask
    (`system/permission_denied`), or ran.
  - update the `//!` usage docs, and keep `cargo build -p delilah-agents --examples`
    green.
- [X] T002 Run the spike from a plain terminal (it spends tokens):
  - `routine` with no mode, and `routine --permission-mode auto`;
  - one PTY run with no flag, to read the `permissionMode` it reports (the terminal
    baseline);
  - one run with an invalid or unavailable mode, to capture the exit code and the exact
    stderr (FR-008).

  Write `docs/spikes/2026-10-26-chat-permission-mode.md`, following the format of
  `docs/spikes/2026-10-24-claude-stream.md`. It must include:
  - the CLI version;
  - a table per mode: action → asked / refused / ran;
  - which control action auto mode escalates, and whether it arrives as `can_use_tool`
    or `permission_denied`;
  - the rejection text;
  - the PTY baseline.

  **Gate**: if the findings contradict research.md R1 (auto mode stops asking for
  routine actions under `--permission-prompt-tool stdio`), stop and revise plan.md
  before Phase 2. Otherwise, note in research.md R1 that the spike confirmed it.

**Checkpoint**: the spike record exists and confirms the mechanism, or the plan was revised.

---

## Phase 2: Foundational, failing tests first, then the core change

**Purpose**: the mode exists end to end in the daemon. It blocks every user story.

### Tests (write first and see them FAIL)

- [X] T003 [P] In the `#[cfg(test)]` module of `crates/delilah-agents/src/claude.rs`,
  add failing tests, next to `permission_modes_map_to_flags`:
  - `Permission::Auto` puts `--permission-mode auto` in both `command(&launch)` (PTY)
    and `stream_command(&launch)` (chat);
  - `Auto` adds neither `--setting-sources` nor `--strict-mcp-config`;
  - `Default` still adds no `--permission-mode`.
- [X] T004 [P] In the tests module of `crates/delilah-e2e/src/fake/stream.rs`, add
  failing tests. With a scenario tool whose `auto` is `"allow"` (the default):
  - a fake launched with `--permission-mode auto` runs it and sends no `can_use_tool`;
  - a fake launched with no mode sends `can_use_tool` as today.

  A tool with `auto: "ask"` sends `can_use_tool` in both cases. Add a parse test for
  `auto` in `crates/delilah-e2e/src/fake/scenario.rs`.
- [X] T005 [P] In the tests module of `crates/delilah-daemon/src/preferences.rs`, add
  failing tests modelled on `the_remote_control_preference_takes_effect_at_once`:
  - `PUT /api/v1/preferences/agent-permissions` with `"default"` makes
    `app.agent_permissions()` return `AgentPermissions::Default` at once, and `"auto"`
    makes it return `Auto`;
  - unset means `Auto`;
  - a PUT of `"bypassPermissions"`, `1`, `null` or `{}` answers 400 and leaves the stored
    value unchanged;
  - other preference keys still accept any JSON.
- [X] T006 [P] In the tests module of `crates/delilah-daemon/src/run.rs`, near line 1790,
  add failing tests for `launch_flags(session, config, AgentPermissions)`:
  - with `Auto`, a task session and a project session get `Permission::Auto`;
  - with `Default`, they get `Permission::Default`;
  - an idea conversation gets `DontAsk` and a read-only session gets `Plan` under both
    values.
- [X] T007 Run `cargo test -p delilah-agents claude::`, `cargo test -p delilah-daemon
  preferences launch_flags` and `cargo test -p delilah-e2e --lib fake::`. Confirm the
  new tests fail (compile errors count as failing), and record the output summary under
  "Red" in `specs/009-chat-mode-permissions/progress.md`.

### Implementation

- [X] T008 [P] In `crates/delilah-agents/src/claude.rs`, add `Permission::Auto`, with a
  doc comment covering what auto mode decides and that it is the user's choice. Map it
  in `shared_args` to `--permission-mode auto`, so both runtimes get it. T003 passes.
- [X] T009 [P] Add `Tool.auto`, a serde enum `allow | ask` with default `allow`, in
  `crates/delilah-e2e/src/fake/scenario.rs`. Honour it in
  `crates/delilah-e2e/src/fake/stream.rs`: skip the `can_use_tool` when `args.rs`
  parsed `permission_mode == Some("auto")` and the tool's `auto` is `allow`. Add a doc
  comment saying these semantics mirror the spike record from T002. T004 passes.
- [X] T010 In `crates/delilah-daemon/src/state.rs`:
  - add `pub enum AgentPermissions { Auto, Default }`;
  - store it in an `Arc<AtomicU8>` field, defaulting to `Auto`;
  - add `agent_permissions()` and `load_agent_permissions()`. The loader reads
    `pref:agent-permissions`; `"default"` gives `Default`, and anything else, including
    unset, gives `Auto`.

  Call `app.load_agent_permissions().await` in `crates/delilah-daemon/src/main.rs`,
  next to `load_remote_control()` at line 66.
- [X] T011 In `crates/delilah-daemon/src/preferences.rs`:
  - turn the `if key == …` reload chain in `put` into a `match key`, with arms for
    `language`, `remote-control` and `agent-permissions`;
  - for `agent-permissions`, validate before saving: only the JSON strings `"auto"` or
    `"default"`, else `ApiError::bad_request`. Then reload.

  Document the 400 on the preference route in `crates/delilah-daemon/src/api.rs`, and
  on `setPreference` in `app/src/api.ts` (add
  `export type AgentPermissions = "auto" | "default"`). T005 passes.
- [X] T012 In `crates/delilah-daemon/src/run.rs`:
  - give `launch_flags` an `AgentPermissions` parameter and map task and project
    sessions to `Auto` or `Default`. Keep it pure, and update its doc comment;
  - pass `app.agent_permissions()` from `spawn_session` (line 1416);
  - update the existing `launch_flags` tests (lines 1796–1819) to the new signature.

  T006 passes.
- [X] T013 Run `cargo test -p delilah-agents`, `cargo test -p delilah-daemon` and
  `cargo test -p delilah-e2e --lib`, plus `cargo clippy --workspace --all-targets -- -D
  warnings` and `cargo fmt --all --check`. All green. Record "Green" in `progress.md`.

**Checkpoint**: a task or project session starts with `--permission-mode auto` by default
in both runtimes, and the preference switches it.

---

## Phase 3: User Story 1, autopilot runs unattended in chat mode (Priority: P1) 🎯 MVP

**Goal**: in chat mode, routine work raises no permission asks. A real ask still pauses
autopilot.

**Independent Test**: the e2e tests below, with the fake agent, through the whole daemon.

- [ ] T014 [US1] Create `crates/delilah-e2e/tests/chat_permissions.rs`, modelled on
  `a_pending_permission_makes_the_task_wait_and_the_answer_runs_it` in
  `crates/delilah-e2e/tests/ui_v2.rs` (helpers `run_in_worktree`, `wait_item`,
  `event_types`; copy what is needed, don't edit ui_v2.rs). Add these failing-first
  tests:
  - (a) `routine_work_in_chat_mode_raises_no_permission_card`: a chat-mode task whose
    fake scenario runs four tools (Write, Bash `sed -i`, Bash `./run.sh`, Bash
    `git commit`, all `auto: "allow"`). The run ends with no `permission` inbox item,
    no `permission-asked` history event, and the fake's record has 0 permissions. The
    fake's recorded argv contains `--permission-mode auto`.
  - (b) `an_action_auto_mode_escalates_still_raises_a_card`: one tool with
    `auto: "ask"` raises a `permission` item, as today.
  - (c) `chat_mode_autopilot_runs_routine_work_and_pauses_only_on_a_real_ask`: a
    chat-mode task started with `"workflow": "feature", "autopilot": true` in the run
    body (the API accepts both; see the `not-spec-workflow` tests in
    `crates/delilah-daemon/src/api.rs`). Its first turn runs the four routine tools: no
    `autopilot-paused` event and no permission item. A later tool with `auto: "ask"`
    produces `autopilot-paused` with "needs permission: Bash" (FR-005, FR-007).
  - (d) `a_runtime_switch_relaunches_with_the_current_mode`: a terminal-mode task
    switched to chat (`POST /api/v1/tasks/{key}/runtime`). The relaunched fake's argv
    has `--permission-mode auto`. Change the preference to `default` with no relaunch:
    the running session's recorded argv is unchanged, and it was not restarted (same
    pid or session record).

  Show that (a) and (d) fail without the fix. Run them on a temporary WIP commit that
  reverts T012's call-site change (pass `AgentPermissions::Default`), see them fail,
  drop that commit, and record the result in `progress.md`.
- [ ] T015 [US1] Start-failure hint (FR-008). In `crates/delilah-daemon/src/run.rs`, next
  to the spawn-failure handling, add one function. It matches the CLI's rejection text
  recorded in the spike (T002) and adds the hint «Ajustes → Permisos de los agentes:
  prueba "Por defecto de Claude"» / "Settings → Agent permissions: try "Claude's
  default"" through `app.lang().pick`. Use it on both runtimes' start-failure paths:
  - chat: where the stream start check (`crates/delilah-daemon/src/stream_events.rs`
    `Started`) reports failure;
  - PTY: a process exit before any work.

  Write the tests first:
  - a unit test of the matcher, with the spike's exact stderr and an unrelated error;
  - one e2e test per runtime in `crates/delilah-e2e/tests/chat_permissions.rs`, using a
    fake scenario that exits at start with that stderr. Add an `exit_with_stderr` step
    to `crates/delilah-e2e/src/fake/scenario.rs` and `run.rs` if no such step exists.

  The user-visible error must contain the hint.

**Checkpoint**: US1 is fully testable. A chat-mode autopilot run of routine work never
pauses for permission.

---

## Phase 4: User Story 2, hands-on chat work, and the setting (Priority: P2)

**Goal**: the user can see and choose the mode. "Claude's default" keeps today's
behaviour exactly. Cards and remembered rules still work.

**Independent Test**: the e2e and vitest tests below, plus the preview check in
quickstart.md.

- [ ] T016 [P] [US2] In `crates/delilah-e2e/tests/chat_permissions.rs`, add
  `claudes_default_brings_back_todays_cards`. Set the preference to `"default"`, then
  run the same four-tool chat scenario as T014(a). Expect four permission asks in turn,
  each answered "allow once" through `/api/v1/sessions/{id}/permissions/{ask}`. The
  fake's argv has no `--permission-mode`. Also add
  `a_remembered_rule_still_answers_without_a_card`: under `"default"`, answer the first
  ask with "always in this task", then expect a second identical command to be answered
  without a card (ADR-001).
- [X] T017 [P] [US2] In `app/src/components/SettingsView.test.tsx`, write failing vitest
  tests:
  - the section «Permisos de los agentes» / "Agent permissions" shows Auto selected
    when the preference is `null` or `"auto"`, and «Por defecto de Claude» when it is
    `"default"`;
  - choosing the other option calls `api.setPreference("agent-permissions", value)`;
  - a failed save shows the error and restores the previous choice.

  Follow the existing `remote-control` tests in that file.
- [X] T018 [US2] In `app/src/components/SettingsView.tsx`, implement the control after
  T017, modelled on the `remote-control` toggle (lines 47–68): a two-option radio or
  segmented control, with a short explanation in both languages. The explanation says
  Auto lets Claude's auto mode decide, so routine work in the workspace does not ask,
  and that it applies to new or relaunched sessions in terminal and chat. Use the app's
  existing i18n mechanism. Run `cd app && npx tsc --noEmit && npx vitest related --run
  src/components/SettingsView.tsx`.

**Checkpoint**: US1 and US2 both work. The user can switch the mode and see the effect
after a relaunch.

---

## Phase 5: User Story 3, proven and kept proven (Priority: P1)

**Goal**: the evidence and the tests agree, and stay that way.

- [ ] T019 [US3] Compare the fake's `auto` semantics (T009) and the e2e expectations
  (T014, T016) with the spike table (T002). If escalations arrive as
  `permission_denied` and not as `can_use_tool`, adjust `Tool.auto`'s `ask` semantics
  and T014(b)/(c) to what was observed, and note it in the spike doc. Record the
  comparison in `progress.md`.
- [ ] T020 [US3] Run the whole suite:
  - `cargo test --workspace`;
  - `cargo test --manifest-path app/src-tauri/Cargo.toml`;
  - `cd app && npx vitest run`;
  - clippy and fmt.

  Everything must be green, including the existing permission-card, remembered-rule and
  autopilot-pause tests (SC-004). Record it in `progress.md`.

---

## Phase 6: Polish and cross-cutting

- [ ] T021 [P] `docs/status.md`:
  - in "Chat mode, limits and known problems", say chat mode now gets the agent
    permission setting (Auto by default) and stops asking for routine work;
  - add the setting to what is built;
  - note the remaining limit: auto mode's own decisions, and an account without auto
    fails at start with a hint.
- [ ] T022 [P] `docs/architecture.md`:
  - the launch flags per session kind (the table from data-model.md);
  - the `agent-permissions` preference and its validation;
  - in the Chat mode section, how the mode reaches both runtimes.
- [ ] T023 [P] `docs/decisions.md`: add the next numbered decision, "Agent permission
  mode is a Delilah setting applied to both runtimes, Auto by default (2026-10-26)". It
  covers:
  - the cause (print mode ignores the interactive default) and the probe numbers;
  - why a setting and not always-auto-in-chat or reading `~/.claude.json`;
  - that the default widens the terminal for users whose Claude default is not auto;
  - that #43 is unchanged.
- [ ] T024 [P] `README.md`: the setting in the Ajustes or chat-mode section, as the user
  sees it.
- [ ] T025 Verify in a preview, following `specs/009-chat-mode-permissions/quickstart.md`
  (`scripts/preview.sh --fresh` in the background; never the installed app). Use the
  `ui-verifier` agent with these scenarios:
  - the setting shows Auto;
  - a chat-mode task doing routine work shows no «¿Permitir?» card;
  - after switching to «Por defecto de Claude» and switching the runtime, cards appear.

  Stop the preview by its process, not by name. Record the verdict in `progress.md`.

---

## Dependencies and execution order

- **Phase 1 (T001 → T002)** is the gate: nothing else starts until the spike confirms R1
  or the plan is revised.
- **Phase 2**:
  - the test tasks T003–T006 run in parallel (different files);
  - T007 runs after all of them;
  - T008 and T009 run in parallel, and T010 → T011 → T012 run in sequence (T011 and T012
    depend on T010's enum);
  - T013 runs last.
- **Phase 3 (US1)** depends on Phase 2. T015 also needs T002's rejection text.
- **Phase 4 (US2)**:
  - T016 depends on Phase 2 and T014's helpers;
  - T017 → T018 can run in parallel with all Rust work after T011, since they need only
    the preference key.
- **Phase 5** follows Phases 3 and 4.
- **Phase 6**: the docs tasks T021–T024 can run in parallel any time after Phase 3. T025
  runs last.

## Parallel examples

```text
# Phase 2 tests, all at once:
T003 claude.rs tests | T004 fake stream/scenario tests | T005 preferences.rs tests | T006 run.rs tests
# Phase 2 implementation:
T008 claude.rs  ||  T009 fake  ||  (T010 → T011 → T012 daemon)
# After T011:
T017 → T018 (app/)  ||  Phase 3 Rust/e2e work
# Docs:
T021 status.md || T022 architecture.md || T023 decisions.md || T024 README.md
```

## Implementation strategy

- **MVP**: Phases 1–3. The spike, the mode in the daemon with Auto as the default, and
  the US1 e2e proof are enough to stop autopilot pausing in chat mode, even before the
  Ajustes control exists (the preference can be set over the API).
- **Then**: Phase 4 (the user-facing setting and the "Claude's default" regression), Phase
  5 (evidence check), Phase 6 (docs and preview).
- Commit per task group, with Conventional Commits in English and no AI attribution.
