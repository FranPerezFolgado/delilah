# Implementation Plan: Saving mode (one fresh session per phase)

**Branch**: `feature/LIBRE-5` | **Date**: 2026-10-21 | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `specs/004-saving-mode/spec.md`

## Summary

A task gets a `saving_mode` flag. In saving mode its sessions form a chain of three
steps (plan → implement → review), each a fresh Claude session in the same workspace.
A new daemon module, `handoff.rs`, runs a small persisted state machine
(`tasks.handoff`): on the primary session's turn end it checks the spec summary;
planning done → offer the handoff in the Bandeja (confirming also approves the tasks);
every task checked → hand over to review on its own. A handoff optionally types
`/save-session` into the outgoing session and waits for that turn, then, in one
transaction, moves `primary` to a new session record with its `step`, records
`StepStarted` in the history, stops the old PTY and launches the new session with the
step's prompt (`/orchestrate specs/<dir>`, or the review prompt). Failures roll back
to the old primary. The UI shows the chain above the session tabs and a "Modo ahorro"
marker in the header; settings hold the defaults. See [research.md](research.md).

## Technical Context

**Language/Version**: Rust 2024 (daemon, core, CLI); TypeScript + React (`app/`)

**Primary Dependencies**: axum, sqlx (SQLite), tokio, portable-pty; React, Vite; vitest + Testing Library

**Storage**: SQLite: migration `0011_saving_mode.sql` (`tasks.saving_mode`, `tasks.handoff`, `sessions.step`); preferences `saving-mode`, `save-notes` in `settings`

**Testing**: `cargo test -p delilah-daemon` (handoff state machine against fake PTY sessions as in `session_api.rs`/`scheduler.rs` tests), `cargo test -p delilah-core`, `cargo test -p delilah-cli`; `cd app && npx tsc --noEmit && npx vitest run` (`chain.test.ts`, `SessionChain.test.tsx`)

**Target Platform**: macOS (daemon + Tauri app); tried in a preview (`scripts/preview.sh`)

**Project Type**: desktop app with a local daemon (API-first)

**Performance Goals**: a handoff starts the new session within 1 minute (SC-002); no new polling (reuses the hook path and the 30 s scheduler for recovery)

**Constraints**: exactly one primary per running task; tasks without saving mode unchanged (FR-013); everything Delilah types to Claude in the user's language; agent settings stay outside the repo

**Scale/Scope**: one new daemon module, one migration, two endpoints, one inbox kind; one helper, one component, changes to the new-task form, task screen, inbox and settings

## Constitution Check

`.specify/memory/constitution.md` is still the unfilled template; the project's
ground rules (`CLAUDE.md`) serve as gates instead:

| Gate | Status |
|---|---|
| Nothing from the work repos; fixtures only | ✅ tests use temp workspaces and fake PTYs; trying it uses fixture projects |
| Agent hooks/settings outside the repo, clean env | ✅ new sessions go through `launch_session` (settings in `tasks/<KEY>/`, clean env) |
| Try changes only in a preview, never 7420 / pkill | ✅ quickstart uses `scripts/preview.sh` |
| No remote operations without confirming the account | ✅ the review prompt forbids push and PR |
| Migrations: never edit a shipped one | ✅ new `0011` |
| Contracts (`api.rs` ↔ `api.ts`) | ⚠️ changed, additively: [contracts/api.md](contracts/api.md); contract review at the end of the phase that touches them |
| Decision 15 ("Delilah never drives SpecKit's phases") | ⚠️ relaxed on purpose: Delilah starts sessions between phases, never acts inside one; new decision entry in `docs/decisions.md` |

Post-design re-check: no change.

## Project Structure

### Documentation (this feature)

```text
specs/004-saving-mode/
├── spec.md
├── plan.md
├── research.md
├── data-model.md
├── quickstart.md
├── contracts/api.md
└── checklists/{requirements,handoff}.md
```

### Source Code

```text
crates/delilah-core/src/task.rs                 # Task.saving_mode, Task.handoff, Handoff/Step/HandoffStage, Event::StepStarted (status unchanged)
crates/delilah-daemon/migrations/0011_saving_mode.sql
crates/delilah-daemon/src/store.rs              # read/write saving_mode, handoff; record StepStarted
crates/delilah-daemon/src/store_sessions.rs     # SessionRecord.step; hand_over(task, from, new) and undo in one transaction; session_of_step
crates/delilah-daemon/src/handoff.rs            # NEW: boundaries, state machine, prompts, start/rollback, recovery, API handlers
crates/delilah-daemon/src/phases.rs             # on_hook: TurnEnded → handoff::on_turn_ended; Working code edit by plan step → skipped; set() forward → handoff::check
crates/delilah-daemon/src/run.rs                # RunRequest.saving_mode (default from pref); first session step=plan, label "Spec y plan", prompt note
crates/delilah-daemon/src/scheduler.rs          # tick: handoff::recover
crates/delilah-daemon/src/inbox.rs              # ItemKind::Handoff items from tasks.handoff
crates/delilah-daemon/src/api.rs                # routes: /tasks/{key}/saving-mode, /tasks/{key}/handoff
crates/delilah-daemon/src/lib.rs                # mod handoff
crates/delilah-cli/src/main.rs                  # --saving-mode / --no-saving-mode on new/run
app/src/api.ts                                  # types + setSavingMode, handoff()
app/src/chain.ts, app/src/chain.test.ts         # NEW: chainOf(task, sessions)
app/src/components/SessionChain.tsx (+ .test)   # NEW: the chain above the tabs, reopen on done steps
app/src/components/TaskScreen.tsx               # chain + "Modo ahorro" marker; follow the new primary's tab
app/src/components/NewTaskDialog.tsx            # "Modo ahorro" checkbox, preloaded from the preference
app/src/components/BacklogTaskView.tsx          # same option for backlog tasks
app/src/components/InboxView.tsx                # handoff item: Implementar en sesión nueva / Seguir aquí / Reintentar
app/src/components/SettingsView.tsx             # the two preferences
app/src/i18n/en.ts                              # English strings
docs/{status,architecture,decisions}.md, README.md
```

**Structure Decision**: the handoff is its own module because it spans hooks,
phases, the store, the inbox and the scheduler; `phases.rs` only calls into it, as it
does for the review hint today.

## Phasing

1. **Core + storage**: types, migration, store methods (one-primary invariant tested at the store).
2. **Handoff engine**: `handoff.rs`, hooks/phases wiring, run.rs first session, scheduler recovery, endpoints, inbox kind, CLI flag.
3. **UI**: api.ts, chain helper + component, task screen, new-task/backlog option, inbox buttons, settings.
4. **Docs**: status, architecture (sessions + handoff), decisions (new entry relaxing 15), README.

## Complexity Tracking

| Choice | Why needed | Simpler alternative rejected because |
|---|---|---|
| Persisted handoff stage | FR-011: restarts and limit pauses mid-handoff | In-memory (like the review hint) loses the offer and can double-start a step |
| Transactional primary switch with rollback | FR-006/FR-014: never zero or two primaries | Kill-then-insert leaves a window with no primary; insert-then-kill with two |
