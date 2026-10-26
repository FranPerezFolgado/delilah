# Research: Saving mode

## R1. Where the handoff state lives

- **Decision**: a nullable `handoff` JSON column on `tasks` (`{ to, stage, error? }`) plus a `step` column on `sessions`. The handoff is a small state machine in a new daemon module `handoff.rs`.
- **Rationale**: FR-011 (safe across restarts and limit pauses) rules out the in-memory set the review hint uses (`AppState.review_hints`). One row per task, written before each side effect, lets the scheduler finish or re-offer a handoff after a restart. `sessions.step` makes "is there already a session for this step?" a query, so starting a step twice is impossible.
- **Alternatives**: a separate `handoffs` table (history is already `task_events`; one live handoff per task is enough); deriving everything from sessions alone (cannot tell "offered" from "saving notes").

## R2. Detecting the boundaries

- **Decision**: check on the primary session's `TurnEnded` (not limited), in `phases::on_hook`, the same place as the review hint; read the spec summary (`specs::read_spec_of`):
  - step `plan` → offer `implement` when `tasks.md` exists, its total > 0 and done == 0;
  - step `implement` → start `review` when total > 0 and done == total.
  - Also check after `POST /tasks/{key}/phase` moves the phase forward (manual phase move offers the handoff).
- **Rationale**: a turn end is the only moment the outgoing session is idle, so typing into it (save notes) or killing it is safe. The 5 s spec watcher alone could fire mid-turn.
- **"Implements anyway"**: when `signal_phase` reports a code edit from the session whose step is `plan`, the task's handoff is set to `{ to: implement, stage: skipped }`: the offer never appears and the task carries on there; the review handoff still applies.
- **Two boundaries at once**: the target is the furthest step reached; a pending `implement` offer becomes a `review` start if every task is already checked.

## R3. Switching the primary session

- **Decision**: a store method `hand_over(task, from, new_record)` that in one transaction sets the old session's `is_primary = 0`, inserts the new one with `is_primary = 1` and `step`, and records `Event::StepStarted` in `task_events`. Then the daemon kills the old PTY and calls `launch_session` with the step's prompt. If launching fails, a second transaction restores the old primary and marks the new record closed, and the handoff goes to `stage: failed` (FR-014).
- **Rationale**: the invariant "exactly one primary per running task" is held by the database, not by timing. `Live.primary` (in `sessions.rs`) is set at insert from the record, so the new live session is primary from its first hook; the old one, killed, no longer sends hooks, and a hook from it would arrive with `primary = false`.
- **Old session**: stays `closed = 0` and stopped, so the existing "Reabrir" brings it back as an extra session (its record is no longer primary).

## R4. Prompts

All typed in the user's language (`lang.pick`), like `continue_prompt`:

| Step | Prompt |
|---|---|
| plan (first session) | the usual `/feature …` plus a note: "Modo ahorro: cuando `tasks.md` esté escrito, para y no implementes; Delilah seguirá en una sesión nueva." |
| save notes | `/save-session` |
| implement | `/orchestrate specs/<dir>` |
| review | `/review` of the branch, then apply the confirmed findings, update the docs it made stale and commit; never push or open a PR |

The spec folder (`SpecSummary.dir`) is resolved by `specs.rs` already.

## R5. Save notes before leaving

- **Decision**: preference `save-notes` (default on). Stage `saving`: type `/save-session` into the outgoing primary with `actions::type_prompt`; the next not-limited `TurnEnded` of that session moves to `starting`. A question (`NeedsUser`/AskUserQuestion) just waits: the inbox already shows it. A `TurnFailed` without limit sets `stage: failed` with retry.
- **Usage limit during it**: the task pauses as today; after the reset `continue_agent` types "carry on" into the primary (still the outgoing one), which finishes the save; its turn end continues the handoff. Nothing new needed.
- **Restart during it**: the PTY is gone; recovery skips the save (the conversation can still be reopened) and goes to `starting` for review, or back to `offered` for implement.

## R6. Defaults and toggling

- Preferences `saving-mode` (bool, default false) and `save-notes` (bool, default true) via the existing `/api/v1/preferences/{key}`.
- `RunRequest.saving_mode: Option<bool>`; when absent the daemon reads `saving-mode` (so the CLI and the Jira start get the installation's default). The new-task form preloads it.
- `POST /api/v1/tasks/{key}/saving-mode { on }` changes it on a running task; off clears a pending handoff.

## R7. The chain in the UI

- **Decision**: a pure helper `chainOf(task, sessions)` in `app/src/chain.ts` → three steps `{ step, label, state: pending|active|done|waiting, session? }`; a `SessionChain` component above the session tabs in `TaskScreen`; a "Modo ahorro" marker next to `PhaseBadge` in the header. The new primary's tab is selected after a handoff (the app follows the primary like today's first open).
- **Inbox**: a new `ItemKind::Handoff` with the offer ("Implementar en sesión nueva" / "Seguir aquí"), the saving/starting state, and failures ("Reintentar"). Notification on the automatic review handoff through `notify`.

## R8. Measuring the saving (SC-001)

The per-message `usage` in Claude's transcripts (`~/.claude/projects/<workspace>/<session>.jsonl`): input + cache read + cache creation per turn, summed over a task's sessions. The quickstart shows the one-liner; no product code.
