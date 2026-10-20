# Research: Task backlog and deleting tasks

## R1. Which state is "backlog"

- **Decision**: the existing `TaskStatus::Queued` is the backlog state. The UI and the
  CLI show it as "Backlog"; the serialized name stays `queued`.
- **Rationale**: `Queued` already means "created, no workspace, no agent", and
  `Event::Start` only applies to it. Today a task is only `Queued` between
  `create_task` and `Event::Start` inside `start_run` (same request, no await on
  anything slow in between), and tasks made with `delilah new` stay in it. That
  is exactly the backlog (spec: tasks created with `delilah new` become backlog tasks).
- **Alternatives**: a new `Backlog` variant. Rejected: it duplicates `Queued`,
  needs a data migration of the JSON status and adds one more transition.
  Renaming the serde tag was rejected because it breaks stored rows.

## R2. Starting an existing backlog task

- **Decision**: `POST /api/v1/runs` with the `key` of an existing `Queued` task
  starts **that** task instead of failing on the duplicate key. The body may change
  the project, submodules, place, prompt and workflow; the title and notes come from
  the task if the body leaves them out. The workspace/agent half of
  `start_run` moves to `launch_task(app, task, opts)` and both paths call it.
- **Rationale**: one door for "start", so the CLI's `delilah run KEY` and the
  Jira view's "Analizar con Claude" for a ticket already in the backlog (spec edge
  case) work with no change on their side.
- **Alternatives**: a separate `POST /tasks/{key}/start`. Rejected: two endpoints
  with the same options, and the Jira view would still have to pick one.
- An existing key that is **not** `Queued` still answers 409, as today.

## R3. A start that fails leaves the task in the backlog

- **Decision**: a new `Event::StartAborted { reason }`: `PreparingWorkspace → Queued`.
  When a task that was already in the backlog fails to get its workspace, `launch_task`
  applies `StartAborted` (instead of `Fail`) and returns the 422. Checks before
  `Event::Start` (project kind, checkout taken, missing prompt) never move the task.
- **Rationale**: FR-005. A brand-new task started directly keeps today's behaviour
  (`Fail`), so nothing changes for the existing flow.
- **Alternatives**: rolling the row back by hand. Rejected: the event history would
  lie about what happened.

## R4. Two starts at once

- **Decision**: `Store::apply_event` opens its transaction with `BEGIN IMMEDIATE`
  (sqlx: `pool.begin_with("BEGIN IMMEDIATE")`), so the read-check-write of
  `Event::Start` is serialized; the second start finds `PreparingWorkspace` and
  gets the usual 409 (`cannot apply start to a task that is preparing-workspace`).
- **Rationale**: with a deferred transaction both readers may see `Queued`; the
  second writer then fails with `SQLITE_BUSY` (a 500), not a readable 409.
- **Alternatives**: an in-memory "starting" set in `AppState`. It works too, but it
  only covers one daemon and adds state; the immediate transaction is one line.

## R5. Notes reach the agent

- **Decision**: `tasks.notes` (nullable). On start, a non-empty note is written
  to `<task_dir>/notes.md` and the prompt gets a pointer to it, like `ticket.md`
  today: `"<prompt> (tus notas sobre la tarea están en <path>)"`, for both the
  `/feature` prompt and a custom prompt.
- **Rationale**: same pattern and same place as the Jira ticket (outside the repo,
  inside `extra_dirs`); the first prompt stays a one-liner even with long notes.

## R6. Deleting a task: order of operations and atomicity

- **Decision** (`DELETE /api/v1/tasks/{key}?force=`), in this order:
  1. Load the task. `PreparingWorkspace` → 409 ("se está preparando…").
  2. If it has a **worktree** and `!force`: run the unpushed/dirty checks
     **without removing anything** (split `archive_workspace` into
     `check_workspace` + `remove_workspace`). Failure → 409 that names `force`. The
     agent is still running.
  3. Stop every live session of the task (`kill` + `wait`), as archive does.
  4. Release the workspace with the same helper as archive
     (`release_workspace`: `leave_checkout` or `remove_workspace`). Failure → 409,
     the task is kept (the agent is already stopped; the user can retry or force).
     A checkout already taken by a newer task has no workspace row: skip.
  5. `Store::delete_task`: a single `DELETE FROM tasks WHERE key = ?`; the
     `ON DELETE CASCADE` foreign keys (enabled in `Store::open`) remove `task_events`,
     `workspaces` and `sessions` (with their `attention`, which feeds the inbox).
  6. Drop the in-memory state keyed by the task: `app.asks`, review hints,
     `app.sessions` entries.
  7. Remove `<task_dir>` (best effort; a failure is logged, not returned).
  8. Emit `Tasks`, `Sessions`, `Inbox`, `Specs`.
- **Rationale**: step 5 is the commit point. Before it the task is fully there;
  after it it is gone everywhere (FR-011, restart edge case). Doing the safe check
  first means declining the warning leaves a running agent untouched.
- **Branch**: `git worktree remove` keeps the branch, and `leave_checkout` doesn't
  touch it (checked in `delilah-git/src/workspace.rs`): FR-010 holds without changes.

## R7. Free-task numbers never repeat

- **Decision**: keep the last number handed out in `settings` (`free_key_last`);
  `next_free_key = max(stored, max existing LIBRE-n) + 1`, then store it.
- **Rationale**: today it is `max existing + 1`, so deleting the newest free task
  would hand out its key again while `feature/LIBRE-n` still exists (`create_workspace`
  would fail on the existing branch). Taking the max with existing keys covers
  databases from before the setting.

## R8. Creating a backlog task

- **Decision**: `POST /api/v1/tasks` (`NewTask`) gains `project` (required),
  `notes`, `free` (and `key` becomes optional with `free`); `workflow` becomes
  optional and defaults to the project's. Editing uses `PATCH /api/v1/tasks/{key}`
  (title, notes, project) and only applies to `Queued` tasks.
- **Rationale**: reuses the endpoint the CLI already calls; `project` is required
  by the clarification. `delilah new` gets `--project` (required) and `--notes`.
- The sidebar already sorts tasks by `created_at` through `list_tasks`
  (`ORDER BY created_at, key`): oldest first for free.
