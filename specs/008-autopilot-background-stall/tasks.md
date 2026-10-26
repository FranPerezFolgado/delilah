# Tasks: Autopilot doesn't pause while the agent waits on background work

**Input**: Design documents from `specs/008-autopilot-background-stall/`
**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/, quickstart.md

**Tests**: requested by the spec (User Story 3, FR-007, FR-008) and test-first: every
story's tests are written and seen failing (or, for US2's regression guards, passing on
the old code for the right reason) before its implementation.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: can run in parallel (different files, no dependency on an unfinished task)
- **[Story]**: US1 / US2 / US3 from spec.md

---

## Phase 1: Setup

**Purpose**: neutral payload fixtures shared by the daemon and agent tests.

- [X] T001 [P] Create neutral `Stop` payload fixtures in `crates/delilah-daemon/tests/fixtures/hooks/`. Write `stop-background-running.json` (one `running` subagent entry, id `a1`, agent_type `implementer`, description `WP-01`), `stop-background-done.json` (two entries, `completed` and `failed`), `stop-background-empty.json` (`background_tasks: []`) and `stop-no-background-field.json` (no field). Use the shape in `contracts/stop-hook.md`; `session_id`/`cwd` are placeholders the tests overwrite. No prompts, messages, paths or ids from real tasks (FR-009).

---

## Phase 2: Foundational (blocks the stories)

**Purpose**: the payload rule, and a fake Claude able to play an autopilot run. The two
parts share no files and run in parallel.

### Payload rule (delilah-agents)

- [X] T002 [P] Write unit tests for `background_running` in `crates/delilah-agents/src/claude.rs` (tests module, beside the `classify` tests). Cover: a running entry → true; one running among finished → true; all `completed`/`failed` → false; `[]` → false; field missing → false; field not an array → false; an entry with no `status` → false. Run them and see them fail (function missing).
- [X] T003 Implement `pub fn background_running(hook: &Value) -> bool` next to `classify` in `crates/delilah-agents/src/claude.rs`, with a doc comment: Claude Code's `Stop` reports `background_tasks`; only `status == "running"` counts; any other shape means nothing is running. Run `cargo test -p delilah-agents background_running` green.

### Fake Claude (delilah-e2e)

- [X] T004 [P] Write tests in `crates/delilah-e2e/src/fake/scenario.rs` (tests module) for the new scenario format. The `stop` string form parses as before. The object form parses `message` plus an optional `background` list. A malformed `stop` (e.g. a number, or an object without `message`) is an error that names the step. The `write` step parses `path` and `content`. `select` prefers `<task>.<command>.json` (prompt `/feature KEY …` → `KEY.feature.json`, `/orchestrate specs/x` → `KEY.orchestrate.json`) over `<task>.json`, falls back to `<task>.json` when it is missing, and skips the lookup for a prompt with no leading `/word`. Run them and see them fail.
- [X] T005 Implement in `crates/delilah-e2e/src/fake/scenario.rs`:
  - `Step::Stop(StopStep)`, with `StopStep::{Text(String), Full { message: String, background: Option<Vec<serde_json::Value>> }}` and a deserializer whose error names the `stop` step;
  - `Step::Write { path: String, content: String }`;
  - the `<task>.<command>.json` lookup in `select` (`<command>` = the prompt's leading `/word` without the slash; the joined name must pass `valid_name`).

  Make T004 green.
- [X] T006 Write tests in `crates/delilah-e2e/src/fake/run.rs` (tests module, using the existing `settings`/`logged` helpers):
  - a `stop` object with `background` logs a `Stop` payload whose `background_tasks` equals the list and whose `last_assistant_message` is the message;
  - the string form logs no `background_tasks`;
  - `write` creates `specs/k-1-x/tasks.md` (parents included) under the fake's cwd with the content;
  - `write` with an absolute path, a `..` component or an empty path prints an error and exits 2.

  Run them and see them fail.
- [X] T007 Implement in `crates/delilah-e2e/src/fake/hooks.rs` `payload::stop_with(session, cwd, message, background: Option<&[Value]>)` (`payload::stop` delegates with `None`), and play `Step::Stop`/`Step::Write` in `crates/delilah-e2e/src/fake/run.rs`. Make T006 green and keep `cargo test -p delilah-e2e --lib` green.
- [X] T008 [P] Update `specs/005-e2e-tests/contracts/fake-claude.md` with the delta in `specs/008-autopilot-background-stall/contracts/fake-claude.md`: the lookup order, the `stop` object form, and the `write` step; add `background_tasks` to the `Stop` row of the payload table.

**Checkpoint**: `cargo test -p delilah-agents` and `cargo test -p delilah-e2e --lib` green.

---

## Phase 3: User Story 1 - Autopilot keeps running while the agent waits on its subagents (P1) 🎯 MVP

**Goal**: a main-session turn end with background work running is never a stall; the
step end and the handoff still happen.

**Independent Test**: the e2e run (T010) goes plan → implement → review with background
waits and no nudge or pause.

### Tests for User Story 1 (write first, see them fail)

- [X] T009 [US1] Add daemon tests that post `stop-background-running.json` (via `include_str!`) through `hooks::apply_hook`, in the tests module of `crates/delilah-daemon/src/hooks.rs`. Set up an autopilot task with a primary session using `autopilot/testkit.rs`; add a `post_hook`-and-wait helper there if needed. Cover:
  - plan step with no tasks.md: autopilot stays `running`, `nudged` is `None`, and there is no `autopilot-nudged` event;
  - the same posted three times: still no nudge or pause;
  - implement step with unchecked tasks: the same;
  - plan step with tasks.md written and nothing checked, posted with background running: the handoff to implement still starts (FR-004).

  Wait for the spawned turn-end work, as the existing hook tests do. Run them and see the first three fail.
- [X] T010 [US1] Create `crates/delilah-e2e/tests/autopilot.rs` with `autopilot_waits_on_background_work`. Use `Env::start`, a fixture repo project and the fake Claude.
  - Scenario `<KEY>.feature.json`: write `specs/<key lowercased>-x/spec.md` and `plan.md` → `stop` with a running `architecture-reviewer` → `sleep_ms` → `stop` with it running again → write `tasks.md` with three unchecked tasks → `stop` with `background: []`.
  - Scenario `<KEY>.orchestrate.json`: `stop` with two running implementers, twice with a sleep between → write `tasks.md` with all tasks checked → `stop` with `[]`.

  Launch the feature task with `autopilot: true` and poll the task and its history (the existing `Api` helpers; add one in `crates/delilah-e2e/src/api.rs` if history isn't reachable) until the review step's session starts. Assert:
  - `autopilot.state` was never `paused` at any poll;
  - no `autopilot-nudged` or `autopilot-paused` events;
  - `step-started` for implement and then review.

  Run it and see it fail on the current daemon (it nudges, then pauses). Record the failure output in `specs/008-autopilot-background-stall/progress.md`.

### Implementation for User Story 1

- [X] T011 [US1] In `crates/delilah-daemon/src/handoff.rs`, add `pub(crate) struct TurnEnd { pub background: bool }` (`Default`) and `on_turn_end(app, key, TurnEnd)` / `on_turn_end_with(app, key, TurnEnd, launch)` holding the body of `on_turn_ended_with`. Keep `on_turn_ended(app, key)` and `on_turn_ended_with(app, key, launch)` as wrappers passing `TurnEnd::default()`. In the no-boundary branch, return before `autopilot::stall` when `end.background`, with a `tracing::debug!`. Add a comment: waiting only skips the stall; it never blocks `finish_save`, the boundary or a handoff.
- [X] T012 [US1] In `crates/delilah-daemon/src/phases.rs` `on_hook`, `Signal::TurnEnded if primary && !limited`: build `TurnEnd { background: delilah_agents::claude::background_running(hook) }` before spawning and call `crate::handoff::on_turn_end`. `maybe_hint` stays after it. Make T009 and T010 green, and keep `cargo test -p delilah-daemon` green.

**Checkpoint**: US1 works on its own. The false pause is gone, and the e2e run reaches review.

---

## Phase 4: User Story 2 - A real stall is still caught (P1)

**Goal**: with nothing running in the background, decision #39 holds unchanged.

**Independent Test**: the e2e real-stall run (T014) gives one nudge, then a pause.

### Tests for User Story 2

- [X] T013 [P] [US2] Add daemon tests in the tests module of `crates/delilah-daemon/src/hooks.rs`, with the T009 setup. Cover:
  - `stop-background-done.json`, `stop-background-empty.json` and `stop-no-background-field.json` on an implement step with unchecked tasks: the first takes the nudge (`nudged == Some(Implement)`, an `autopilot-nudged` event); a second pauses with the "N tasks unchecked" reason;
  - running ×3 then empty: the empty one nudges and does not pause;
  - running and empty posted together (`tokio::join!`): exactly one `autopilot-nudged` event;
  - a `StopFailure` payload with a running `background_tasks` entry still pauses (FR-005).

  These are regression guards: they pass both before and after T011–T012, so check that each one asserts the state it names.
- [X] T014 [P] [US2] Add `autopilot_still_catches_a_real_stall` to `crates/delilah-e2e/tests/autopilot.rs`. The scenario `<KEY>.feature.json` writes spec, plan and tasks with all tasks unchecked, and launches implement. The scenario `<KEY>.orchestrate.json` does `stop` with `[]` → `wait_input` → `stop` with `[]`. Assert, in order:
  - exactly one `autopilot-nudged` (implement), then `autopilot-paused`;
  - `autopilot.state == "paused"`, with a reason that names the unchecked tasks;
  - the fake's record (`<fake-dir>` record file, as the existing fake tests read it) shows the nudge text among its inputs.

**Checkpoint**: US1 and US2 both green.

---

## Phase 5: User Story 3 - Tests that reproduce the real run (P2)

**Goal**: the new tests prove they would have caught the bug, and the fixtures are clean.

**Independent Test**: T010 fails on the code from before T011–T012 and passes after.

- [ ] T015 [US3] Prove SC-003. Check out the branch at the commit before T011 into a temporary worktree (`git worktree add`, then remove it), or revert T011–T012 locally without committing, and run `cargo test -p delilah-e2e --test autopilot autopilot_waits_on_background_work`. Confirm it fails with the nudge or pause, then confirm it passes on the branch head. Record both outcomes in `specs/008-autopilot-background-stall/progress.md`.
- [ ] T016 [P] [US3] Check that the fixtures and the scenarios written by T001, T010 and T014 hold only field names and neutral values. That means no prompts, assistant messages, real ids, user paths or repo names: grep `crates/delilah-daemon/tests/fixtures/hooks/` and `crates/delilah-e2e/tests/autopilot.rs` for `/Users/`, `LIBRE-`, `PROJ-` and hex ids longer than 8 characters. Fix any hit.

---

## Phase 6: Polish & cross-cutting

- [X] T017 [P] Add a sentence to decision #39 in `docs/decisions.md`: a turn that ends with background tasks running (`Stop` hook `background_tasks`, `status: "running"`) is not a stall. Say the field is Claude Code's, observed rather than documented, that an unknown shape falls back to the stall rule, and point to `specs/008-autopilot-background-stall/contracts/stop-hook.md`.
- [X] T018 [P] Update `docs/status.md`: the fix (autopilot no longer pauses while subagents run), the new fake-claude `stop`/`write` steps and per-command scenarios, and the autopilot e2e test.
- [X] T019 [P] Add one line to the autopilot section of `README.md`: autopilot waits while the agent's background subagents run, and only nudges or pauses when the agent has really stopped.
- [ ] T020 Run every check: `cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace` (docs/development.md). Fix any failures inside this feature's files.

---

## Dependencies & Execution Order

- **Phase 1 (T001)** → T009 and T013 (the fixtures).
- **Phase 2**: T002→T003 (agents) and T004→T005→T006→T007 (fake) run in parallel; T008 can run at any time.
- **US1**: T009 needs T001 and T003 (to compile the call in its setup, if any). T010 needs T005 and T007. T011→T012 come after T009 and T010 have been seen failing.
- **US2**: T013 needs T001; T014 needs T007. Both can be written in parallel with US1's tests, but are checked green after T012.
- **US3**: T015 needs T012; T016 needs T001, T010 and T014.
- **Polish**: T017–T019 at any time after T012; T020 last.

### Parallel opportunities

- T001 ∥ T002 ∥ T004 ∥ T008 (different crates and files).
- T013 ∥ T014 (daemon tests module and the e2e test file are different files; T014 adds to T010's file, so it runs after T010).
- T017 ∥ T018 ∥ T019.

### Parallel example (Phase 2)

```text
Agent A: T002 → T003   (crates/delilah-agents/src/claude.rs)
Agent B: T004 → T005 → T006 → T007   (crates/delilah-e2e/src/fake/*)
Agent C: T001, T008    (fixtures, contract doc)
```

## Implementation Strategy

- **MVP**: Phases 1–3. The false pause is fixed and shown fixed by an e2e run.
- Then US2 locks down the rule that must not change, US3 proves the tests can fail, and Polish closes with the docs. No version bump: the user bumps and releases when they choose.
- Commit per work package, as `/orchestrate` does.
