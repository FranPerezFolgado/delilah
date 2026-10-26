# Research: autopilot and background work

## R1. What tells the daemon the agent is waiting on background work

- **Decision**: the `background_tasks` array of Claude Code's `Stop` hook payload. The
  turn waits on background work when at least one entry has `status == "running"`.
- **Evidence** (shape only, from a real `hooks.jsonl`, LIBRE-15): every `Stop` carried
  `background_tasks`; while subagents ran each entry was
  `{"id": "<hex>", "type": "subagent", "agent_type": "<name>", "description": "<text>", "status": "running"}`,
  and with nothing in flight the list was `[]`. Each false pause happened 3–4 s after a
  nudge, on a `Stop` whose list had running subagents.
- **Rationale**: it is what the agent itself reports, on the very hook that drives the
  stall check; nothing has to be guessed from the transcript or timing.
- **Unknown statuses** (anything other than `running`) do not count as running. That is
  only the observed value, and it fails safe: an unknown state gives today's behaviour
  (one nudge, then a pause), never an autopilot that waits forever on a status it can't read.
- **Kinds**: any `type` counts (subagent, background shell…). Claude wakes the session
  when any of them ends.
- **Alternatives considered**:
  - A grace period after each turn end before stalling: it guesses, delays real stalls, and
    a subagent can run for many minutes.
  - Reading the transcript for "waiting for…": fragile and language-dependent.
  - Unlimited nudges: ruled out by decision #39 (they could loop).

## R2. Where the check goes

- **Decision**: `phases::on_hook` already has the payload on `TurnEnded`. It reads
  `claude::background_running(hook)` and passes it to `handoff::on_turn_ended`, whose
  no-boundary branch skips `autopilot::stall` when it is true.
- **Rationale**: the step-end check (boundary → handoff) still runs on every turn end
  (FR-004); only the stall is skipped. `stall` stays a pure policy over the task state.
  Parsing the payload stays in `delilah-agents`, as `classify` does.
- **Alternatives considered**: a new `Signal::TurnEndedWaiting`. Every `TurnEnded` match
  (hooks, inbox, live state, conversation) would have to learn it, and for all of them
  except the stall it means the same as `TurnEnded`.
- **Resume**: `autopilot::resume` re-evaluates the chain through `on_turn_ended_with`
  without a hook; it passes "no background work" (today's behaviour).

## R3. Ruled out: the handoff's kill of the old session

`try_start` stops the plan session after `hand_over` has made the implement session the
main one, so a late `StopFailure`/`Stop` from the old session reaches `on_hook` with
`primary == false` and never drives `on_turn_failed`/`stall`. LIBRE-15's history has no
pause at the handoff; its pauses are all stall reasons ("tasks.md not written",
"N tasks unchecked").

## R4. Tests that behave like a real run

- **Decision**: extend the existing fake Claude (`crates/delilah-e2e`, contract in
  `specs/005-e2e-tests/contracts/fake-claude.md`) and add an autopilot end-to-end test
  against the isolated daemon (`Env`). The fake gets:
  - a `stop` step in object form, with a `background` list sent as `background_tasks`;
  - a `write` step that writes a file under its cwd (spec/plan/tasks, ticking tasks);
  - a per-command scenario lookup `<task>.<command>.json` (`/feature` → `feature`,
    `/orchestrate` → `orchestrate`): the plan and implement sessions of one task have to
    play different scripts, and the step prompts have no `[fake:…]` marker.
- **Rationale**: the bug lives in the chain hook → classification → turn end → stall,
  across a real handoff and a second session. Only a run that goes through the real hook
  endpoint, the real handoff and real sessions shows it. The daemon tests that call
  `stall()` with hand-built state missed it because they never see a payload.
- **Daemon-level tests** additionally post recorded payload shapes through the hook
  endpoint (fast, no processes): running, all finished, list absent.
- **Fixtures**: payload shapes only, with neutral values (`"id": "a1"`,
  `"agent_type": "implementer"`, `"description": "WP-01"`). No prompts, messages or paths
  from real tasks (ground rule: nothing from the work repos).
- **Alternatives considered**: replaying a whole real `hooks.jsonl`. It holds real
  prompts, tool output and paths, so it can't go in the repo, and it is tied to one run's
  timing.
