# Data model: Task name and branch from the first analysis

Migration `crates/delilah-daemon/migrations/0015_task_branch_from_spec.sql` (see
[research.md](research.md) R6).

## Task (`tasks`)

| Field | Type | Notes |
|---|---|---|
| `title_source` | `TEXT NOT NULL DEFAULT 'manual'` | `initial` · `spec` · `manual`. New free tasks start `initial`; Jira tasks and every task that existed before the migration are `manual`. |
| `previous_title` | `TEXT NULL` | The title the automatic rename replaced; cleared by any manual title change. |
| `branch_error` | `TEXT NULL` | Why the branch couldn't be created/applied; cleared when `ensure` succeeds. |

`delilah_core::Task` gains `title_source: TitleSource`, `previous_title: Option<String>`,
`branch_error: Option<String>` (all serialised in the API).

### `title_source` transitions

```
initial ──(spec.md has a real heading)──▶ spec      (previous_title = old title)
initial ──(user changes the title)──────▶ manual
spec    ──(user changes or restores)────▶ manual    (previous_title = NULL)
manual  ── terminal: never renamed automatically
```

## Workspace (`workspaces`)

| Field | Type | Notes |
|---|---|---|
| `branch` | `TEXT NOT NULL` | `''` while the task has no branch (`Workspace::has_branch()`); set once by `branching::ensure`, never changed after. |
| `spec_dirs_at_start` | `TEXT NULL` | JSON array of the spec folder names present at start (main repo and chosen submodules, as `label:name`); NULL for workspaces started with a branch. |
| `previous` | (existing) | Checkout mode; for parent projects now also holds the parent itself (`"."`). |

### Branch lifecycle

```
(start, free task) ── '' ──┬─ hook switched the main repo to X ─────▶ X
                           ├─ new spec folder NNN-name, no branch ──▶ NNN-name[-k]
                           └─ guard, no spec folder ────────────────▶ slug(title)[-k]
then (parent) checkout -b <branch> in each chosen submodule; failures → branch_error
```

`delilah_git::WorkspaceSpec.branch: Option<String>` (`None` = start without one).

## Analysis (not stored)

The first spec folder that didn't exist at start: `specs/<name>/spec.md` in the main
repo or a chosen submodule. Gives the branch name (when Delilah creates it) and the
title (its `# Feature Specification: <name>` heading).
