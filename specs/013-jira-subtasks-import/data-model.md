# Data model: Jira sub-tasks become Delilah subtasks

## Stored

### `jira_subtask_skips` (new, migration `0026_jira_subtask_skips.sql`)

| Column | Type | Notes |
|---|---|---|
| `parent_task` | TEXT NOT NULL | key of the parent task |
| `jira_key` | TEXT NOT NULL | Jira sub-task the user skipped |
| `created_at` | TEXT NOT NULL | RFC 3339 |

Primary key `(parent_task, jira_key)`. No foreign key (as decision 102). Rows are removed when
the parent task is deleted, or one by one on "unskip". Archiving keeps them.

### `tasks` (unchanged)

A parent task from Jira is a `kind = 'parent_task'` row keyed by the ticket; each subtask from
Jira a `kind = 'subtask'` row keyed by the Jira sub-task with `parent_task` set. No new column.

## Computed (daemon, never stored)

### `ImportPlan` (pure function `plan_import`)

Inputs: the ticket (`JiraIssue`), the Delilah task with its key (if any) and whether it has an
open PR or a session, the ticket's Jira sub-tasks, the Delilah tasks with those keys (with
`started` and archived flags), the skips of the parent task, and the set of sub-task keys
assigned to the current user.

```
ImportPlan {
  ticket: { key, title, status, is_subtask },
  parent: Create | Convert | Existing | Refused { code, message },
  subtasks: [SubtaskRow],
  gone: [ { key, title } ],      // Delilah subtasks of the parent task not under the ticket in Jira
}

SubtaskRow {
  key, title, status, status_category, assignee: Option, mine: bool,
  task: Option<{ key, kind, status, parent_task, archived }>,
  action: Create | Attach | Linked | Conflict { reason },
  skipped: bool,
  selected: bool,                // default selection
}
```

Rules:

| Case | `action` |
|---|---|
| no Delilah task with the key | `Create` |
| normal task, never started, same project (or project not yet set), `check_attach` ok | `Attach` |
| subtask of this parent task (live or archived) | `Linked` |
| anything else (started, other project, other parent task, a parent task) | `Conflict { reason }` |

`selected` = `action ∈ {Create, Attach}` ∧ `!skipped` ∧ `status_category != "done"` ∧
(`assignee == None` ∨ `mine`).

`parent`:

| Delilah task with the ticket key | `parent` |
|---|---|
| none | `Create` (a ticket that is a Jira sub-task → `Refused { nesting }`) |
| normal, `check_make_parent_task` ok | `Convert` |
| parent task | `Existing` |
| otherwise | `Refused` with spec 012's code |

### `ImportResult`

`{ parent_task: key, created: [key], attached: [key], left_out: [{ key, reason }], skipped:
[key], unskipped: [key] }`.
