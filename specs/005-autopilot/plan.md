# Implementation Plan: Autopilot (a task that runs on its own up to the PR)

**Branch**: `feature/LIBRE-9` | **Date**: 2026-10-22 | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `specs/005-autopilot/spec.md`

## Summary

A task gets an `autopilot` state (nullable JSON on `tasks`, implies saving mode). It does
not add a second engine: saving mode's `handoff.rs` keeps owning the step sessions, and
asks a new `autopilot.rs` how to shape each turn end. With autopilot *running*, the
planning → implementation offer becomes an automatic start; a turn end that reaches no
boundary gets one nudge, then pauses; the end of the review reads an outcome file the
review session writes (`ready`/`blocked`, PR title and body) and, when ready, starts the
project's preview (a new daemon-owned process, `preview.rs`) and opens a draft PR through
a reusable `open_for` extracted from `pr_api/open.rs`, after re-detecting PRs so it is
never opened twice. Questions with an option marked "(Recommended)" are answered in the
AskUserQuestion hook; anything else (question without one, permission prompt, failure)
pauses into a new `autopilot` Bandeja item with *Reanudar* / *Seguir a mano*. Projects
gain a `preview_command`, and any task can start/stop its preview by hand. The UI adds
the option next to saving mode, a header mark, the chain's two final steps, the preview
controls, the project field and "Crear y lanzar en autopiloto" on the idea card. See
[research.md](research.md).

## Technical Context

**Language/Version**: Rust 2024 (daemon, core, CLI); TypeScript + React (`app/`)

**Primary Dependencies**: axum, sqlx (SQLite), tokio (`process` for the preview), portable-pty; React, Vite; vitest + Testing Library

**Storage**: SQLite migration `0015_autopilot.sql` (`tasks.autopilot` JSON, `projects.preview_command`); review outcome files in `~/.delilah/tasks/<KEY>/autopilot/`; previews in memory

**Testing**: `cargo test -p delilah-core`; `cargo test -p delilah-daemon` (stub `Launcher` and fake PTYs as in `handoff.rs` tests, `gh::fake::FakeGh`, preview with tiny shell commands); `cargo test -p delilah-cli`; `cd app && npx tsc --noEmit && npx vitest run`

**Target Platform**: macOS (daemon + Tauri app); tried in a preview

**Project Type**: desktop app with a local daemon (API-first)

**Performance Goals**: an automatic handoff starts within the saving-mode bound (1 minute); preview URL detected as soon as printed, timeout 10 minutes; no new polling (turn-end hooks, the existing 30 s scheduler tick for recovery)

**Constraints**: tasks without autopilot unchanged (SC-006); at most one PR per repo (SC-003); never answers permission prompts; agent sessions keep a clean env (the preview is not an agent and runs in a login shell); everything told to Claude or the user in the app's language

**Scale/Scope**: two new daemon modules (`autopilot.rs`, `preview.rs`), one migration, ~6 endpoints, one inbox kind, five history events; UI changes in ~9 components

## Constitution Check

`.specify/memory/constitution.md` is still the unfilled template; the ground rules in
`CLAUDE.md` are the gates:

| Gate | Status |
|---|---|
| Nothing from the work repos; fixtures only | ✅ tests use temp workspaces, fake PTYs, fake `gh`; trying uses fixture projects |
| Agent hooks/settings outside the repo, clean env | ✅ sessions still go through `launch_session`; outcome files live in `tasks/<KEY>/`, outside the repo |
| Try changes only in a preview; never 7420, never `pkill` by name | ✅ quickstart uses `scripts/preview.sh`; previews started by the daemon are stopped by process group, never by name |
| Remote operations only as the confirmed account | ⚠️ autopilot pushes and opens a PR without asking: it uses the user's own `gh`/git exactly as "Abrir PR" does; the PR is a draft; the user opted in per task. Recorded as a decision |
| Migrations: never edit a shipped one | ✅ new `0015` |
| Contracts (`api.rs` ↔ `api.ts`) | ⚠️ changed, additively: [contracts/api.md](contracts/api.md); contract review at the end of the phases that touch them |
| Decisions 15/33 (Delilah never acts inside a phase) | ⚠️ stretched: one typed nudge after a stall, and in-hook answers for recommended options. Delilah still never drives the workflow's phases; new decision entry |

Post-design re-check: no change.

## Project Structure

### Documentation (this feature)

```text
specs/005-autopilot/
├── spec.md
├── plan.md
├── research.md
├── data-model.md
├── quickstart.md
├── contracts/api.md
└── checklists/{requirements,chain}.md
```

### Source Code

```text
crates/delilah-core/src/task.rs                 # Autopilot, AutopilotState/Stage; Task.autopilot; Event::Autopilot{Paused,Resumed,Answered,Nudged,Finished} (status unchanged)
crates/delilah-core/src/project.rs              # Project.preview_command
crates/delilah-daemon/migrations/0015_autopilot.sql
crates/delilah-daemon/src/store.rs              # read/write autopilot (forcing saving_mode), clear on saving-mode off
crates/delilah-daemon/src/store_projects.rs (or wherever projects are stored)  # preview_command
crates/delilah-daemon/src/autopilot.rs          # NEW: shape(turn end), stall/nudge, pause/resume/advance, recommended(), finish (outcome → preview → PR), recover, API handler
crates/delilah-daemon/src/handoff.rs            # consult autopilot::shape; Boundary::Finish for the review; autopilot prompts next to plan_note/step_prompt
crates/delilah-daemon/src/preview.rs            # NEW: Previews registry, start (zsh -lc, process group, URL capture, timeout), stop, API handlers
crates/delilah-daemon/src/pr_api/open.rs        # extract open_for(app, key, repo, title, body, draft); handler uses it
crates/delilah-daemon/src/asks.rs               # answer recommended options in the hook; Answer.launch
crates/delilah-daemon/src/proposals.rs          # "Crear" + launch=autopilot → create and start
crates/delilah-daemon/src/inbox.rs              # ItemKind::Autopilot from tasks.autopilot; permission attention → autopilot::pause
crates/delilah-daemon/src/run.rs                # RunRequest.autopilot; first prompt with the autopilot note
crates/delilah-daemon/src/api.rs, projects.rs   # routes: /tasks/{key}/autopilot, /tasks/{key}/preview; NewTask/BacklogChange.autopilot; PATCH project preview_command
crates/delilah-daemon/src/transitions.rs, actions.rs  # stop the preview on done/fail/archive/delete
crates/delilah-daemon/src/scheduler.rs          # tick: autopilot::recover (stage opening_pr)
crates/delilah-daemon/src/state.rs, main.rs, lib.rs   # Previews in AppState; stop all on shutdown; mods
crates/delilah-cli/src/main.rs                  # --autopilot/--no-autopilot; `autopilot <KEY> on|off|resume`
scripts/preview.sh                              # UI line before API; PREVIEW_URL line
app/src/api.ts                                  # types + setAutopilot, preview calls, project field, answer launch
app/src/chain.ts (+ test)                       # preview and PR steps when autopilot is on
app/src/components/NewTaskDialog.tsx, BacklogTaskView.tsx  # "Autopiloto" option (ticks and locks saving mode)
app/src/components/TaskScreen.tsx (header)      # autopilot mark (running/paused/done + reason); preview URL and Lanzar/Parar
app/src/components/InboxView.tsx                # autopilot item: Reanudar autopiloto / Seguir a mano
app/src/components/ProposalCard.tsx             # "Crear y lanzar en autopiloto"
app/src/components/ProjectView.tsx              # preview command field
app/src/i18n/en.ts                              # English strings
docs/{status,architecture,decisions,development}.md, README.md
```

**Structure Decision**: `autopilot.rs` holds the policy (when to start, nudge, pause,
finish) and `handoff.rs` keeps the mechanism (sessions, persisted handoff, rollback), so
saving mode's code paths are unchanged when `autopilot` is `None`. The preview is its own
module because it is useful without autopilot (manual start on any task).

## Phasing

1. **Core + storage**: types and events, migration, store methods (autopilot ⇒ saving mode; saving mode off ⇒ autopilot cleared), project field.
2. **PR and preview building blocks**: `open_for` extraction (existing PR tests keep passing), `preview.rs` with its endpoints and stop hooks, project PATCH, `scripts/preview.sh` output.
3. **Autopilot engine**: `autopilot.rs`, handoff integration (start instead of offer, stall/nudge, review finish), prompts, outcome file, finish → preview → PR, recovery, pause/resume endpoint, inbox item, permission pause, recommended answers in `asks.rs`, `RunRequest`/backlog fields, CLI.
4. **UI**: api.ts, option in dialog/backlog, header mark and preview controls, chain steps, inbox actions, project field, idea card button + daemon `launch`.
5. **Docs + version**: status, architecture (autopilot + preview), decisions (autopilot policy, remote operations, nudge), development (preview command), README; minor version bump.

## Complexity Tracking

| Choice | Why needed | Simpler alternative rejected because |
|---|---|---|
| Persisted `stage` for the final actions | SC-003: a restart between `gh pr create` and the link must not open a second PR | Memory only: a crash re-opens; a lock file is not safer than the row |
| One nudge per step | The workflow's approval stops are plain text; without it nearly every plan step pauses | No nudge (autopilot rarely finishes); unlimited nudges (can loop) |
| Daemon-owned preview process | Clarified: someone must stop it with the task | Agent-started preview outlives its session with no owner |
| Outcome file per review session | A readable, language-independent end of the review | Parsing the last message (prose, bilingual) |
