# Data model: Parent tasks and subtasks

## Task (core, `crates/delilah-core/src/task.rs`)

New fields (serde default, so old JSON and old rows read as normal tasks):

| Field | Type | Default | Notes |
|---|---|---|---|
| `kind` | `TaskKind` | `normal` | `normal` \| `parent_task` \| `subtask` |
| `parent_task` | `Option<TaskKey>` | `None` | set iff `kind == subtask` |

Invariants (checked in `hierarchy.rs`, enforced on create/convert/attach):

- `kind == subtask` ⇔ `parent_task.is_some()`.
- The referenced task exists, has `kind == parent_task` and is not archived.
- A `parent_task` never has a `parent_task` (two levels only).
- `kind` never goes back: `normal → parent_task` and `normal → subtask` (attach) only.
- Parent task and subtask: `in_checkout == false`, never branchless.
- Parent task: `workflow = none`, `saving_mode = false`, `autopilot = false`,
  `ci_autofix = false` (no agent runs phases on it).
- Subtask: `project` = the parent task's project.

## Derived (daemon, never stored)

- **archived(t)** = `t.status` terminal ∧ no workspace row.
- **merged(t)** = t has ≥ 1 PR ∧ every PR is `Merged`.
- **dropped(t)** = archived(t) ∧ ¬merged(t).
- **live(t)** = ¬archived(t).
- **Progress** of a parent task over its existing subtasks:
  `counted = |subtasks| − |dropped|`, `merged = |merged subtasks|`, `dropped = |dropped|`.
  `ready = counted > 0 ∧ merged == counted`.

## ProjectSettings (core, `project.rs`)

| Field | Type | Default |
|---|---|---|
| `parent_task_merge` | `MergePreference` (`merge` \| `squash` \| `rebase`) | `merge` |

Stored inside the existing `projects.settings` JSON; no migration.

## Migration `0025_task_hierarchy.sql`

```sql
-- Parent tasks and subtasks (spec 012). Existing rows are normal tasks.
ALTER TABLE tasks ADD COLUMN kind TEXT NOT NULL DEFAULT 'normal';
ALTER TABLE tasks ADD COLUMN parent_task TEXT;
CREATE INDEX tasks_parent_task ON tasks(parent_task);
```

No foreign key (the `tasks` table has none today and deletes are by key); deleting a
subtask needs nothing on the parent task; deleting a parent task is refused while it has
live subtasks, and its archived subtasks keep the dangling key (shown as "parent task
deleted" if ever read).

## Lifecycle

```text
Parent task: Backlog ─start→ PreparingWorkspace ─WorkspaceReady→ Running (no agent)
             Running ─MarkDone (PR merged, or by hand)→ Done ─archive (no live subtasks)→ archived
Subtask:     as a normal task; start refused until the parent task has a workspace and
             origin/<parent task branch> exists.
```

## Inbox

New notice kind `parent-task-live`: "PR of <KEY> merged, but it has N live subtasks: not
archived". New notice kind `parent-task-ready`: "All subtasks of <KEY> merged: mark its PR
ready" — pushed once per parent task, when progress first becomes ready while its PR is a
draft.
