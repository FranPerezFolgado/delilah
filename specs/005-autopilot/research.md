# Research: Autopilot

## R1. Where the autopilot state lives

- **Decision**: one nullable JSON column `tasks.autopilot` (migration `0015_autopilot.sql`), read into `Task.autopilot: Option<Autopilot>`. `NULL` = off. `Autopilot { state, stage, reason, since }` (see data-model). Saving mode keeps its own column; autopilot on forces `saving_mode = 1` in the same statement.
- **Rationale**: same shape as `tasks.handoff` (JSON, written before each side effect, read back on restart); one column keeps "on/off" and "where it is" atomic.
- **Alternatives**: a boolean plus a separate state column (two writes that can disagree); keeping the state only in memory (loses a pause or a half-opened PR on restart, against SC-003).

## R2. Driving the chain: reuse the handoff engine

- **Decision**: autopilot does not get its own turn-end hook. `handoff::on_turn_ended` asks `autopilot::shape(task, boundary)`: with autopilot *running*, `Boundary::Offer(Implement)` becomes `Boundary::Start(Implement)` (FR-006), and "no boundary" at a turn end becomes a **stall** (R5). The end of the review step is new: `Boundary::Finish` when the primary step is `Review` and its turn ends; `autopilot::finish` reads the review outcome (R4) and runs the final actions (R6, R7).
- **Rationale**: saving mode already owns the step sessions, the persisted handoff, `save-session`, recovery and rollback. Autopilot only changes who confirms and adds what happens after the review.
- **Alternatives**: a parallel state machine in `autopilot.rs` listening to the same hooks (two owners of the primary session; double starts).

## R3. Making each step run without stops

- **Decision**: the instruction each step's session gets, built in `handoff.rs` next to `plan_note`/`step_prompt`, in the app's language:
  - Plan: `/feature <KEY>` + an autopilot note: answer the workflow's setup questions yourself (markdown, keep the current branch, lean scope with tasks, no implementation), approve every phase yourself, do not ask the user anything that has a recommended option (mark one "(Recommended)" when you ask), stop once `tasks.md` is written.
  - Implement: `/orchestrate <dir>` + note: do not stop between phases or packages, apply fixes without asking.
  - Review: the saving-mode review prompt + run the project's checks + write the PR title and description (e.g. with `pr-writer`) and the outcome file (R4); never push or open a PR.
- **Rationale**: the user's skills are global (`~/.claude/skills`) and outside the repo; the session prompt already overrides their stops in saving mode ("stop once tasks.md is written"). Questions that still come through are answered by R8 when marked as recommended.
- **Alternatives**: an `--autopilot` argument in the skills (edits the user's global skills, outside this repo; may be done later and the prompt keeps working); typing approvals into the PTY on a timer (fragile).

## R4. The review outcome

- **Decision**: the review session writes `~/.delilah/tasks/<KEY>/autopilot/review-<session-id>.json`: `{ "outcome": "ready"|"blocked", "reason"?: string, "pr_title"?: string, "pr_body"?: string }`. The prompt gives the exact path (the task folder is already an extra working directory of the session). On the review turn end Delilah reads the file of the **primary review session**; missing or unparsable = *blocked*, reason "the review did not report an outcome".
- **Rationale**: per-session file name means a stale file from an earlier review can never be read as the current one; the task folder is outside the repo, so nothing leaks into the PR.
- **Alternatives**: parsing the agent's last message (prose, language-dependent); a hook/tool call (needs a new MCP tool).

## R5. Stalls and the single nudge

- **Decision**: a turn end of the primary session with autopilot *running* and no boundary reached is a stall. The first stall of a step gets **one** automatic nudge typed into the same session ("Autopilot: carry on without asking; pick the recommended option yourself; if you truly cannot go on, say why and stop."); a second stall in the same step pauses with a step-specific reason (plan: "tasks.md not written", implement: "N tasks unchecked", review: outcome *blocked* + its reason, never nudged). The nudge count lives in `Autopilot.nudged: Option<Step>`.
- **Rationale**: the spec workflow's approval stops are plain text, not AskUserQuestion; without a nudge most plan steps would pause on their first approval request. One nudge bounds the cost (one extra turn) and keeps FR-013's guarantee: a step that still cannot go on pauses. Recorded as FR-013b in the spec.
- **Alternatives**: no nudge (pauses on every text approval; measured risk on LIBRE-3's transcript: 3 approval stops); unlimited nudges (loops).

## R6. Opening the PR exactly once

- **Decision**: `pr_api/open.rs` gains `open_for(app, key, repo, title, body, draft) -> Result<PrRow, ApiError>` used by both the HTTP handler and autopilot. Autopilot: write `stage = opening_pr` first, then `pr_loop::check_task(app, key, true)` (links a PR that a previous attempt created but did not link), then for each repo with commits beyond its base and no open PR, `open_for(..., draft = true)`. After success: `state = done`, history `AutopilotFinished { prs }`. Restart with `stage = opening_pr` re-runs the same idempotent sequence from the scheduler tick.
- **Rationale**: the linked-PR check plus detection by branch covers the window "gh created it, we crashed before linking" (SC-003).
- **Alternatives**: calling the HTTP endpoint internally (axum extractors, no shared guard); a lock file (does not survive a crash any better than the persisted stage).
- **Body**: the agent's `pr_body` if present, else `proposal()`'s body (Jira link + commit subjects); a preview URL, if running, is appended as a "Preview" line (FR-019).

## R7. The preview process

- **Decision**: new module `preview.rs`: `Previews` registry in `AppState` (task → child). Start = `tokio::process::Command` `/bin/zsh -lc "<preview_command>"` with `current_dir` = the task's workspace root, its own process group (`process_group(0)`), stdout+stderr piped into a 200-line ring buffer. The first `http(s)://` URL seen sets `state = running` and the URL; exit or no URL within **10 minutes** = `failed` with the last lines (FR-018a). Stop = SIGTERM to the group, SIGKILL after 5 s. Stopped on `MarkDone`/`Fail`, archive, delete (hooks in `transitions.rs`, `actions.rs`) and on daemon shutdown; not adopted after a restart (state is memory-only: a restart shows no preview).
- **Login shell**: the command needs the user's PATH (cargo, npm), which launchd's daemon env lacks; `zsh -l` gives it. Agents keep their clean env; previews are not agents.
- **Timeout**: 10 minutes, not 2: a first preview in a fresh worktree builds the daemon and runs `npm ci`.
- **`scripts/preview.sh`**: already non-interactive and picks the first free slot; change only the summary so the UI line is printed before the API line (the first URL is the one to open), and print `PREVIEW_URL=<url>` as its first URL line for clarity. Suggested project setting for this repo: `scripts/preview.sh`.
- **Alternatives**: a PTY like agent sessions (no terminal needed; pipes are simpler); letting the agent start it (rejected in clarification: nobody owns it afterwards).

## R8. Answering questions with the recommended option

- **Decision**: in `asks::hook`, before registering the ask: if the task's autopilot is *running* and the asking session is the task's primary step session, `autopilot::recommended(&questions)` returns the answers map when every question has exactly one option whose label ends in "(Recommended)"/"(Recomendado)" (case-insensitive, trimmed). Then the hook returns the answer at once (no `AgentAsked`, no inbox item) and records `AutopilotAnswered { question, answer }` per question. Otherwise autopilot pauses (reason: the first question's text) and the ask goes on as today.
- **Rationale**: the hook already blocks the tool until Delilah answers; answering in-line avoids a WaitingForUser flicker and a notification.
- **Permission prompts**: `inbox::attention_change` with `AttentionKind::Permission` on an autopilot task pauses it (reason: "the session is waiting for a permission"); autopilot never answers them.

## R9. Pause, resume and the Bandeja

- **Decision**: pause = `state = paused`, `reason`, `since`; history `AutopilotPaused { reason }`; one notification (skipped when the app is listening, like `announce`). The inbox derives an `Autopilot` item from `task.autopilot` while paused (like `Handoff` from `tasks.handoff`), with actions *Reanudar autopiloto* / *Seguir a mano*. Resume = `state = running`, `nudged = None`, then `autopilot::advance`: if the primary session is working, wait for its turn end; else evaluate as at a turn end (boundary → start next step; review done → read outcome and finish; `opening_pr` → reopen). "Seguir a mano" = `autopilot = NULL` (saving mode stays).
- **Manual saving-mode actions while paused** (confirm/decline/retry, Abrir PR) work unchanged because autopilot only acts while *running*.

## R10. Start from an idea

- **Decision**: `POST /asks/{id}/answer` gains optional `launch: "autopilot"`. `proposals::on_answer` with "Crear" + launch creates the task with `autopilot` and `saving_mode` on and then starts it through the same path as starting a backlog task (`run::start_run` with the task's key), in the conversation's project and the project's default work mode. A failed start leaves the backlog task with autopilot on and the error on the card (CardOutcome error, as today).

## R11. Testing without LIBRE-7

- Daemon: the handoff tests' stub `Launcher` and fake PTY sessions, plus `gh::fake::FakeGh` for the PR; outcome files written by the test; preview with a tiny shell command (`echo http://127.0.0.1:9; sleep 30`) and one that exits.
- App: vitest for the chain helper (preview/PR steps), the autopilot badge/banner, the inbox item, the proposal button.
- One manual end-to-end run in a preview with real Claude on a fixture project (quickstart).
