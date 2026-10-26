# Research: CI autofix

## R1. Where autofix decides

- **Decision**: a pure `autofix::decide(task, row, snapshot, idle) -> Action` called by
  `autofix::evaluate(app, key)` at the end of `pr_loop::check_task` (every 30 s tick and
  every push nudge), from `handoff::on_turn_ended` for the primary session, and from the
  toggle route.
- **Rationale**: the PR loop already has the freshest snapshot and runs often; the turn
  end is the moment the agent becomes idle (and when "no push" can be judged). No new
  timer, no new `gh` polling. A usage-limit resume needs no hook: the next tick or turn
  end re-evaluates.
- **Alternatives**: inside `notices::record` when a `ci-failed` notice is raised (the code
  map's first idea) — rejected: the notice is raised as soon as one check fails while
  others still run (`ci_outcome`: failure beats pending), it is raised once per sha (a
  busy agent would miss it), and `record` runs under the check guard where typing into a
  session would hold it. A separate timer — rejected: duplicates the tick.

## R2. "Every check finished"

- **Decision**: eligible when the head's checks contain at least one `Failure` and no
  `Pending` (`Outcome` of `prs::Check`); neutral/cancelled/skipped already map to
  neither and are ignored; no checks at all is never eligible.
- **Rationale**: FR-003 and acceptance 1.3; avoids a fix based on half the picture and a
  second attempt for the same sha when the slower check fails later.

## R3. When the agent is idle

- **Decision**: idle = the task is not `PausedByLimit`, not terminal, and either its
  primary live session is in `LiveState::Waiting` with no pending attention of kind
  `Permission` and no open ask card, or it has no live primary session but a
  resumable (not closed) primary record (relaunched with the prompt, as `forward` does).
  `Working`/`Starting` wait.
- **Rationale**: a turn end leaves the session `Waiting` with a `Question` attention
  (the agent reports and waits) — that is idle for our purpose. A permission prompt or an
  AskUserQuestion card is the user's to answer (FR-017); typing into it would answer it.
- **To verify while implementing**: how an open AskUserQuestion card shows on the
  session (pending ask in `asks.rs`) so it is excluded.

## R4. One sending path

- **Decision**: extract from `pr_api::forward::forward` a
  `send(app, key, prompt) -> Result<(), ApiError>` (PausedByLimit refusal, type into the
  live primary or relaunch the record). The manual route and autofix both use it. The
  autofix prompt is `ci_prompt` plus an autofix header ("Delilah's CI autofix, attempt
  N of 3") and a tail ("commit and push the fix to this same branch; don't force-push or
  open another PR; if you can't fix it, say why and stop").
- **Rationale**: same framing as today ("data, not instructions from the user"), same
  size caps (`fit`, `safe_url`, `one_line`), no second implementation to drift.

## R5. Per-PR state and idempotency

- **Decision**: `pull_requests.autofix` JSON: `{state, attempts, sha, session, sent_at,
  reason}`; every change through `Store::update_pr_autofix(id, expected, next)`, a
  compare-and-set, written **before** typing the prompt. `sha` is the last head
  forwarded (by autofix or by hand): a head equal to it is never sent again.
- **Rationale**: SC-002 across restarts (the state is written first; a crash after the
  write and before typing loses at most one attempt, never doubles it — preferred over
  sending twice). Mirrors autopilot's `update` (decision 45). Per PR because a parent
  has one PR per repo with its own count (edge case "several repos").
- **Alternatives**: a column on `tasks` — rejected (per-PR counts); a table of attempts —
  rejected for state (history already records each attempt).

## R6. Outcomes of an attempt

- Head changes and is green (`ci_outcome == Success`) → `watching`, `attempts = 0`,
  history `autofix-fixed`.
- Head changes and is red (eligible) → next attempt if `attempts < 3`, else `stopped`
  (`limit`).
- Primary turn ends while `fixing`: force a `check_task` first (the push nudge may not
  have run), then if the head still equals `sha` → `stopped` (`no-push`). If it changed,
  stay `fixing` until its CI finishes.
- PR merged/closed, task archived/deleted/backlog, toggle off → state cleared to
  `watching`/off; nothing sent.
- **Stopped** raises an `autofix-stopped` notice (cause = `<sha>:<reason>`) via the
  existing `pr_notices` (unique per cause, resolved not deleted, decision 28), one
  notification. It is resolved by a green head, retry, toggle, or merge/close.

## R7. Notifications for CI failures

- **Decision**: `notices::record` skips the "CI failed" notification for a task with
  `ci_autofix` on whose PR autofix isn't `stopped` (the notice row is still written, so
  the Inbox item exists until autofix sends and resolves it).
- **Rationale**: FR-012 — the user hears only when autofix gives up.

## R8. Defaults

- **Decision**: `ProjectSettings.ci_autofix: AutofixDefault` with serde default
  `Autopilot`. At task creation (`POST /tasks`, backlog create, idea "Create and launch
  on autopilot"): explicit `ci_autofix` wins; else `Always` → on, `Autopilot` → on iff
  the task has autopilot, `Off` → off. Turning autopilot on later doesn't change
  `ci_autofix` (it is the task's own setting once created). Migration backfills
  `ci_autofix = 1` where `autopilot IS NOT NULL`.
- **Rationale**: spec assumption; keeps the setting simple and explicit per task.

## R9. Decision log

- **Decision 87** (new): "CI autofix forwards CI failures on its own, opt-in and
  bounded" — amends #29 for CI failures only; reviews stay manual; rationale and limits.

## R10. Plan review fixes (architecture-reviewer, 2026-10-26)

- **Task-level, serialised evaluate (HIGH 2, 3).** `evaluate(app, key)` runs **after**
  `check_task` drops its `CheckGuard` (spawned, never under it) and takes a per-task
  `AutofixGuard` (a set of keys in `AppState`, like `pr_loop::claim`; a second caller
  returns at once — the next tick or turn end re-evaluates). It reads every PR row of
  the task, runs `decide()` per row, and builds **one** prompt for all rows eligible to
  send (one section per PR), CASes each row to `fixing`, then sends once. This is how
  the spec's "several PRs ready at the same time go in one message" holds.
- **Turn-end judgement that can't be skipped (HIGH 1).** `on_turn_ended` spawns a task
  that runs `pr_loop::check_task_waiting(app, key)` — waits for the claim (bounded,
  e.g. 60 s) instead of returning — and judges `no-push` only from a snapshot whose
  `checked_at` is after the turn end. If no such snapshot can be had (claim timeout,
  `gh` error), the state stays `fixing` and the next turn end or tick decides (a tick
  never judges `no-push`; only a turn end does).
- **Attribution of the turn (HIGH 4).** A turn end counts for `no-push` only if it is
  of `PrAutofix.session` and the turn started after `sent_at` (the primary session's
  turn start is known from the `UserPromptSubmit`/activity timestamp; if not available,
  the first turn end after `sent_at`). A user turn in the same session after the
  autofix turn is still "the agent's turn" for this purpose; accepted.
- **Leaving `fixing` on every exit (HIGH 4).** Toggle off, PR merged/closed, task
  archived/deleted/back to backlog: CAS `fixing` → `watching` and record
  `autofix-cancelled`. A head that changed and stays pending simply waits (as the PR
  loop does); nothing is sent meanwhile.
- **A failed send doesn't count (HIGH 5).** If `send` fails after the CAS, CAS each row
  back to its previous value (attempt and sha restored); a `no-session` error instead
  stops with reason `no-session`.
- **Manual forward through the CAS (MEDIUM 6).** `POST /pr-notices/{id}/forward` for a
  `ci-failed` notice first CASes the PR's `sha` to the notice's cause (creating the
  JSON from NULL with defaults); if autofix already holds that sha it resolves the
  notice and returns 204 without sending again.
- **Where types live (MEDIUM 8).** `PrAutofix` stays in `delilah-daemon/src/autofix/`
  (serde for the column); core gets only `Task.ci_autofix` and `AutofixDefault`.
- **Upgrade (MEDIUM 10).** The migration turns autofix on only for non-terminal tasks
  with autopilot, and seeds each of their open PRs with `sha` = the current head
  (`json_extract(snapshot, '$.head')`), so the upgrade never fires at an old failure;
  noted in `docs/status.md`.
- **Contract (MEDIUM 7, 13).** No summary field on `GET /prs` (badge uses
  `Task.ci_autofix`, see scope cuts); body is `{"action":"on"|"off"}` or
  `{"action":"retry","repo"}`; `on` resets attempts of every stopped PR. New TS fields
  are optional; `InboxView` keeps a fallback for unknown kinds.
- **Scope cuts (LOW 12).** No worst-of aggregate on `GET /prs` (the badge mark comes
  from `Task.ci_autofix` and the PR tab shows per-PR state); no `autofix-on/off` history
  events; no `delilah show` change; `prompt.rs` folded into `autofix/mod.rs`;
  `AutofixControl` is the one component reused by PR tab, dialogs and backlog.
  Kept: `pr-autofix` inbox kind (it needs "Retry autofix", which `pr-failed` doesn't
  have) and `no-session` (a spec edge case).
