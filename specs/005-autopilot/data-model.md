# Data Model: Autopilot

## `tasks.autopilot` (migration `0015_autopilot.sql`, nullable JSON)

`Task.autopilot: Option<Autopilot>` in `delilah-core/src/task.rs`. `None` = off.

```rust
pub struct Autopilot {
    pub state: AutopilotState,          // running | paused | done
    #[serde(default)] pub stage: Option<AutopilotStage>, // where the final actions are
    #[serde(default)] pub reason: Option<String>,        // why it paused (user-facing, app language)
    pub since: DateTime<Utc>,           // when `state` last changed
    #[serde(default)] pub nudged: Option<Step>,          // the step that already got its one nudge
    #[serde(default)] pub review_session: Option<String>, // the review session whose outcome file the finish reads (set when the review starts)
}
pub enum AutopilotState { Running, Paused, Done }
pub enum AutopilotStage { Preview, OpeningPr }
```

Rules:
- `autopilot` set ⇒ `saving_mode = 1` (set in the same `UPDATE`). Setting `saving_mode = 0` clears `autopilot` (FR-002).
- Only spec-workflow tasks (those that accept saving mode) accept it (FR-001).
- Old rows load with `autopilot = None` (`#[serde(default)]` on the field; a round-trip test as for `handoff`).

State transitions:

| From | Trigger | To |
|---|---|---|
| (off) | user turns on (new, backlog, running, CLI, idea card) | running |
| running | stall after the nudge, step failed, permission prompt, unanswerable question, review *blocked*, uncommitted changes, push/PR failure | paused (+reason) |
| running | review *ready* | running, stage = preview (if the project has a preview command) → opening_pr |
| running, stage = opening_pr | every repo with commits has a linked PR | done |
| paused | "Reanudar autopiloto" | running (`nudged = None`), then `advance` |
| running / paused | "Seguir a mano" / user turns it off / saving mode off | (off) |
| done | — | stays done (shown as "PR abierta por autopiloto"); turning it off clears it |

The usage limit never changes `autopilot` (FR-017): the task status goes to `paused-by-limit` and back.

## History events (`delilah-core::Event`, status unchanged)

| Event | Fields | When |
|---|---|---|
| `AutopilotPaused` | `reason` | every pause |
| `AutopilotResumed` | — | the user resumes |
| `AutopilotAnswered` | `question`, `answer` | a question answered with its recommended option |
| `AutopilotNudged` | `step` | the one nudge of a step |
| `AutopilotFinished` | `prs: Vec<String>` (URLs) | the PRs are linked |

Like `StepStarted`, they leave the status untouched in `TaskStatus::apply`.

## `projects.preview_command` (same migration, nullable TEXT)

`Project.preview_command: Option<String>` in `delilah-core/src/project.rs`; empty string normalised to `None`. Run with `zsh -lc` in the task's workspace root.

## Review outcome file (not in SQLite)

`~/.delilah/tasks/<KEY>/autopilot/review-<session-id>.json`, written by the review session:

```json
{ "outcome": "ready", "pr_title": "feat: …", "pr_body": "…" }
{ "outcome": "blocked", "reason": "cargo test fails in delilah-daemon after 3 attempts" }
```

Missing/invalid ⇒ *blocked* with reason "the review did not report an outcome".

## Preview (memory only, `preview.rs`)

```rust
pub struct PreviewView {          // serialised for the API
    pub state: PreviewState,      // starting | running | failed
    pub url: Option<String>,
    pub error: Option<String>,    // with the last lines of output
    pub started_at: DateTime<Utc>,
}
```

One per task at most. Lost on daemon restart (not adopted again). Changes emit `Topic::Tasks`.
