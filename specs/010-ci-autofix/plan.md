# Implementation Plan: CI autofix

**Branch**: `010-ci-autofix` | **Date**: 2026-10-26 | **Spec**: [spec.md](spec.md)
**Input**: Feature specification from `specs/010-ci-autofix/spec.md`

## Summary

A task gets a `ci_autofix` flag (default from a new project setting, `off` /
`autopilot` / `always`, default `autopilot`). Each PR row gets an `autofix` JSON
state (`watching` / `fixing` / `stopped`, attempts, the last forwarded sha, the
session it went to). A new daemon module `autofix/` holds a pure `decide()` over
(task, PR row, snapshot, agent idleness) and an `evaluate(app, key)` that runs at the
end of every `pr_loop::check_task` (after its guard is dropped, under a per-task
autofix guard), at every primary turn end and when the user toggles (a usage-limit
resume is picked up by the next tick or turn end). It is task-level: all PRs eligible
at once go in one message. When the PR's head has every check finished and at
least one failed, the head sha was not forwarded yet, the attempt count is under 3 and
the agent is idle, it writes `fixing` (compare-and-set, before the side effect), types
the CI prompt into the primary session (or relaunches it) through the same path as
"Ask the agent to fix it" (refactored into one `pr_api::forward::send`), resolves the
`ci-failed` notice and records history. A green head ends the attempt (`fixed`, count
reset); a red new head is the next attempt; a turn end with the head unchanged (after a
forced check) or the limit stops it with an `autofix-stopped` notice, one notification
and "Retry autofix". The manual forward of a `ci-failed` notice also records the sha as
forwarded. The app gets the toggle (task screen PR tab, new task dialog, backlog), the
project setting, a state line in `PrView`, a mark on `PrBadge` and the inbox item. The
CLI gets `delilah autofix <KEY> on|off|retry`, `--autofix/--no-autofix` and
`delilah project set <name> ci-autofix …`. Decision #29 is amended (decision 99).
See [research.md](research.md).

## Technical Context

**Language/Version**: Rust (edition 2024) + TypeScript/React (Tauri app)

**Primary Dependencies**: axum, sqlx (SQLite), tokio, serde_json; the user's `gh` (already used by the PR loop; nothing new is called)

**Storage**: SQLite (`~/.delilah`): migration `0022_ci_autofix.sql` adds `tasks.ci_autofix` (bool, backfilled on for tasks with autopilot) and `pull_requests.autofix` (JSON, NULL = never acted). `ci_autofix` in `projects.settings` JSON (no migration, decision 52). New notice kind `autofix-stopped` in `pr_notices`.

**Testing**: `cargo test -p delilah-daemon` (pure `decide()` table tests, store tests, an in-process flow test with `FakeGh` and a `cat` session as in `pr_api/forward.rs`); `cargo test -p delilah-core` (settings serde/default); e2e `crates/delilah-e2e/tests/autofix.rs` (fake Claude + fake `gh`); vitest for `PrView`, `PrBadge`, `NewTaskDialog`, `ProjectSettings`, `InboxView`.

**Target Platform**: macOS (daemon + Tauri app)

**Project Type**: daemon + CLI + desktop web UI (existing workspace)

**Performance Goals**: an eligible failure is sent within one PR tick (30 s) of its last check finishing; no extra `gh` calls (autofix reads the snapshot the loop already took; the only extra call is the forced check at an attempt's turn end).

**Constraints**: at most one message per head sha (also across restarts and manual forwards); 3 attempts per PR; state written before side effects (compare-and-set, as autopilot, decision 45); never forward review text (decision 29 stays for reviews); prompts in the app's language.

**Scale/Scope**: one or a few open PRs per task; one primary session per task.

## Constitution Check

`.specify/memory/constitution.md` is the unfilled template; the gates are the project's
ground rules in `CLAUDE.md` and `docs/decisions.md`:

| Gate | Status |
|---|---|
| Never edit a shipped migration; new `NNNN_*.sql` | PASS: `0022_ci_autofix.sql` |
| API ↔ `app/src/api.ts` contract kept in step | PASS: [contracts/api.md](contracts/api.md) lists both sides |
| Decisions recorded | PASS: decision 99 amends #29 (CI only, opt-in, bounded) |
| Agent hooks outside the repo, clean env | PASS: no new hooks; reuses the existing Stop hook path |
| Try changes in a preview, never the installed daemon | PASS: quickstart uses `scripts/preview.sh` |
| Docs updated with behaviour | PASS: status, usage, architecture, decisions tasks |
| No version bump on the branch | PASS |

Post-design re-check: PASS (no new crate, no new process, no new credential).

## Project Structure

### Documentation (this feature)

```text
specs/010-ci-autofix/
├── plan.md
├── research.md
├── data-model.md
├── quickstart.md
├── contracts/api.md
├── checklists/requirements.md
└── tasks.md            (speckit-tasks)
```

### Source Code (repository root)

```text
crates/delilah-core/src/
├── project.rs          # ProjectSettings.ci_autofix: AutofixDefault (off|autopilot|always), serde default autopilot
└── task.rs             # Task.ci_autofix: bool; Event variants autofix-sent/-fixed/-failed-again/-stopped/-cancelled

crates/delilah-daemon/
├── migrations/0022_ci_autofix.sql
└── src/
    ├── autofix/
    │   ├── mod.rs      # PrAutofix, decide() (pure), evaluate(app, key) under AutofixGuard,
    │   │               # on_turn_ended, the prompt wrapper, rollback on send failure
    │   └── api.rs      # POST /tasks/{key}/autofix (on|off|retry)
    ├── pr_loop/mod.rs  # check_task → spawn autofix::evaluate after the guard drops; check_task_waiting
    ├── pr_loop/notices.rs # skip the "CI failed" notification when autofix will handle it
    ├── pr_api/forward.rs  # extract send(app, key, prompt); manual ci-failed forward CASes the sha first
    ├── pr_api/dto.rs   # PR DTO gains `autofix`
    ├── store_prs.rs    # read/CAS pull_requests.autofix; autofix-stopped notice kind
    ├── store.rs (tasks)# ci_autofix column read/write; set at creation from project default
    ├── handoff.rs      # on_turn_ended → autofix::on_turn_ended
    ├── inbox.rs        # autofix-stopped item (Retry autofix / Dismiss)
    ├── state.rs        # AutofixGuard (per-task set, like pr_loop's claim)
    ├── actions.rs / archive.rs # archive, delete, back to backlog → autofix::cancel
    └── api.rs          # route; ci_autofix on task create, backlog create/PATCH

crates/delilah-cli/src/main.rs  # `autofix <KEY> on|off|retry`, --autofix/--no-autofix, project set ci-autofix
crates/delilah-e2e/tests/autofix.rs

app/src/
├── api.ts              # Task.ci_autofix, PrAutofix, ProjectSettings.ci_autofix, setAutofix()
├── components/PrView.tsx, PrBadge.tsx, NewTaskDialog.tsx, BacklogTaskView.tsx,
│   ProjectSettings.tsx, InboxView.tsx (+ new AutofixControl.tsx)
└── i18n/{es,en}/       # strings
```

**Plan review** (architecture-reviewer): 5 HIGH fixed in [research.md](research.md#r10-plan-review-fixes-architecture-reviewer-2026-10-26) —
evaluate outside the check guard and serialised per task, task-level batching, a turn-end
check that waits for the claim and only trusts a fresh snapshot, every exit leaves
`fixing`, a failed send rolls back. MEDIUM/LOW applied: manual forward through the CAS,
`PrAutofix` kept in the daemon, upgrade seeding, contract tidy-ups, scope cuts.

**Structure Decision**: a new `autofix/` module next to `autopilot/` and `pr_loop/`,
because it is a policy over both (it reads PR snapshots and drives the agent session),
mirroring how `autopilot/` sits over saving mode. `pr_loop` and `handoff` only call
`autofix::evaluate` / `on_turn_ended`; they don't learn its rules.

## Complexity Tracking

None: one column per table, one JSON state, one module, no new process or credential.
