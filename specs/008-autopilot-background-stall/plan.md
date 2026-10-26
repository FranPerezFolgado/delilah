# Implementation Plan: Autopilot doesn't pause while the agent waits on background work

**Branch**: `008-autopilot-background-stall` | **Date**: 2026-10-26 | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `specs/008-autopilot-background-stall/spec.md`

## Summary

A main-session turn end on autopilot is a stall only when no background task is running.
The daemon reads `background_tasks` from Claude Code's `Stop` payload
(`delilah_agents::claude::background_running`). `phases::on_hook` passes that flag to
`handoff::on_turn_ended`, which still checks the step end every time but skips
`autopilot::stall` while background work runs. The fake Claude learns to end turns with
background tasks, write spec files, and pick a script per step command. A new e2e test
then drives a real plan → implement autopilot run through the isolated daemon and
reproduces the bug before the fix. Daemon tests post recorded payload shapes through the
hook endpoint.

## Technical Context

**Language/Version**: Rust (edition and toolchain of the workspace)

**Primary Dependencies**: existing only (serde_json, tokio, axum in the daemon). No new crates.

**Storage**: none changed (no migration).

**Testing**: `cargo test -p delilah-agents`, `-p delilah-daemon`, `-p delilah-e2e` (the e2e
tests build and run `fake-claude` and a daemon in a temp root).

**Target Platform**: macOS (daemon); the e2e harness runs on the dev Mac.

**Project Type**: daemon + CLI + desktop app; this change is daemon-only plus test tooling.

**Performance Goals**: n/a (one JSON lookup per `Stop`).

**Constraints**: the stall rule of decision #39 stays the same when nothing runs in the
background. No content from real tasks or work repos in fixtures. No UI change.

**Scale/Scope**: ~4 production functions touched, 1 new helper, fake-claude steps, 1 new e2e test file.

## Constitution Check

The constitution file is the unfilled template; the gates are CLAUDE.md's ground rules:

| Gate | Status |
|---|---|
| Nothing from work repos; fixtures are neutral shapes | Pass (FR-009, R4) |
| Agent hooks outside the repo / clean env | Unaffected (the fake reads `--settings` as today) |
| Try changes in a preview, never the installed daemon (7420) | Pass (e2e uses its own temp daemon; quickstart uses `scripts/preview.sh`) |
| Migrations never edited | n/a (no migration) |
| Contracts `api.rs` ↔ `app/src/api.ts` | Unchanged |
| Version bump per Spec Kit task | Patch (bug fix, no new behaviour or API) |

Re-check after design: still passing; the only contract that changes is the internal
fake-claude scenario format, in a backwards-compatible way.

## Design

### 1. Payload rule (`crates/delilah-agents/src/claude.rs`)

`pub fn background_running(hook: &Value) -> bool`, next to `classify`: true iff
`hook["background_tasks"]` is an array with an entry whose `status` is `"running"`
(research R1). Unit tests: running entry; all `completed`/`failed`; `[]`; missing;
not an array; one running among finished ones.

### 2. Turn end (`crates/delilah-daemon/src/phases.rs`, `handoff.rs`)

- `handoff.rs` gets a small `pub(crate) struct TurnEnd { pub background: bool }`
  (`Default` = nothing running) and new entry points
  `on_turn_end(app, key, TurnEnd)` / `on_turn_end_with(app, key, TurnEnd, launch)` that
  hold today's body. `on_turn_ended(app, key)` and `on_turn_ended_with(app, key, launch)`
  keep their signatures and call them with `TurnEnd::default()`. So the resume paths
  (`pause.rs:282`, `:299`) and the ~45 existing handoff tests stay untouched, and no call
  site carries a bare `false`.
- `phases::on_hook`, `Signal::TurnEnded if primary && !limited`: build
  `TurnEnd { background: claude::background_running(hook) }` before spawning and call
  `handoff::on_turn_end`.
- In the no-boundary branch:
  `if end.background { tracing::debug!(task = %key, "turn ended waiting on background work"); return; }`
  before `autopilot::stall`. Code comment: waiting only skips the stall. It never blocks
  `finish_save`, the boundary or a handoff, so a step that ends while something still
  runs in the background still hands over (FR-004).
- `maybe_hint`, spawned after the turn end, stays as it is. It is the "move to review?"
  offer once the branch has commits, not a pause or wait state, so it doesn't touch
  SC-004.
- `on_turn_failed`, limited turns, non-main sessions: untouched (FR-005).

### 3. Daemon tests through the hook endpoint

In `crates/delilah-daemon` (beside the existing hook tests, using `autopilot/testkit.rs`):
an autopilot task in the plan step with no tasks.md (and one in implement with unchecked
tasks) and a primary session. `Stop` payloads go through `hooks::apply_hook`, the code
behind `POST /hooks/claude`, from fixtures in
`crates/delilah-daemon/tests/fixtures/hooks/` (`stop-background-running.json`,
`stop-background-done.json`, `stop-no-background-field.json`; neutral values). Wait for
the spawned turn-end work, then assert:
- running: `autopilot.state == running`, `nudged == None`, no `autopilot-nudged` event,
  and repeating it changes nothing;
- done / absent: the step's nudge is taken; a second one pauses with the step's reason;
- running then empty: the first empty one nudges (it does not pause);
- two `Stop`s posted at once, one running and one empty: exactly one nudge;
- a `StopFailure` while background work runs still pauses (edge case, FR-005).

The fixtures are read with `include_str!` from the in-crate tests module, because
`apply_hook` and the testkit are `pub(crate)`. A testkit helper to post a fixture and wait goes in `autopilot/testkit.rs` if needed.

### 4. Fake Claude (`crates/delilah-e2e/src/fake/{scenario,run,hooks}.rs`)

Per [contracts/fake-claude.md](contracts/fake-claude.md):
- `Step::Stop` accepts the string or the object form (enum
  `StopStep { Text(String), Full { message, background: Option<Vec<Value>> } }`, with a
  deserializer whose error names the step; a malformed `stop` has its own test);
  `payload::stop_with(session, cwd, message, background)` adds `background_tasks`.
- `Step::Write { path, content }`, path checked (relative, no `..`), parents created.
- `scenario::select` tries `<task>.<command>.json` after the marker.
- Unit tests in the same modules, as the existing ones (`run.rs` tests with `logged`).
- Update `specs/005-e2e-tests/contracts/fake-claude.md` with the delta.

### 5. E2E test (`crates/delilah-e2e/tests/autopilot.rs`)

With `Env::start`, a fixture repo project, the fake Claude:
- `autopilot_waits_on_background_work`: scenarios `<KEY>.feature.json` (write spec.md and
  plan.md → stop with a running reviewer → sleep → stop with it running again → write
  tasks.md with unchecked tasks → stop with `background: []`) and `<KEY>.orchestrate.json`
  (stop with two running implementers twice → write tasks.md all checked → stop with
  `[]`). Launch the task with `autopilot: true`. Poll the task and history until the
  review step's session starts; assert `autopilot.state` was never `paused`, there are no
  `autopilot-nudged`/`autopilot-paused` events, and `step-started` implement then review
  appear. The spec folder is `specs/<key lowercased>-x/` in the workspace, which
  `find_spec_dir` finds by key.
- `autopilot_still_catches_a_real_stall`: `<KEY>.orchestrate.json` (or plan) stops with
  `[]` and the step unfinished, `wait_input` (gets the nudge), stops with `[]` again;
  assert one `autopilot-nudged`, then `autopilot-paused` whose reason names the unchecked
  tasks, and that the nudge text reached the fake (`record.inputs`).
- Test-first: written and run before part 2 lands; the first test fails on the current code.

### 6. Docs and version

- `docs/decisions.md` #39: one sentence. A turn that ends with background tasks running
  (the `Stop` hook's `background_tasks`) is not a stall. Add that the field is defined by
  Claude Code, observed rather than documented, and that an unknown shape falls back to
  the stall rule.
- `docs/status.md`: the fix, and the new fake-claude steps.
- `README.md` autopilot section: one line, autopilot waits while the agent's subagents
  run. This is user-facing behaviour (CLAUDE.md: the README covers what the user sees).
- Patch bump with `scripts/bump-version.sh patch --commit`.

## Project Structure

### Documentation (this feature)

```text
specs/008-autopilot-background-stall/
├── spec.md
├── plan.md
├── research.md
├── data-model.md
├── quickstart.md
├── contracts/
│   ├── stop-hook.md
│   └── fake-claude.md
└── tasks.md            # /speckit-tasks
```

### Source Code (files touched)

```text
crates/delilah-agents/src/claude.rs                 # background_running + tests
crates/delilah-daemon/src/phases.rs                 # pass the flag on TurnEnded
crates/delilah-daemon/src/handoff.rs                # TurnEnd + on_turn_end(_with); skip stall
crates/delilah-daemon/src/autopilot/testkit.rs      # helper to post a hook fixture (if needed)
crates/delilah-daemon/src/hooks.rs                  # tests through apply_hook (tests module)
crates/delilah-daemon/tests/fixtures/hooks/*.json   # neutral Stop payload shapes
crates/delilah-e2e/src/fake/scenario.rs             # stop object form, write step, lookup
crates/delilah-e2e/src/fake/run.rs                  # play them
crates/delilah-e2e/src/fake/hooks.rs                # payload::stop_with
crates/delilah-e2e/tests/autopilot.rs               # new e2e tests
specs/005-e2e-tests/contracts/fake-claude.md        # contract delta
docs/decisions.md, docs/status.md, README.md
```

**Structure Decision**: no new modules. The payload rule lives next to `classify` in the
agent adapter, the policy stays in `handoff`/`autopilot`, and the tooling goes in the
existing e2e crate.

## Work order and parallelism

- A (agents rule) and C (fake-claude steps) share no files: parallel.
- D (e2e test) needs C; it is written and run against the unfixed daemon (it must fail).
- B (daemon wiring + hook tests) needs A.
- E (docs, bump) last.

## Complexity Tracking

None. No gate violations.
