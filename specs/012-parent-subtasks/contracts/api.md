# Contracts: Parent tasks and subtasks

All changes are additive (FR-029): new optional request fields, new response fields, new
endpoints. Mirror every type in `app/src/api.ts`.

## Types

```ts
type TaskKind = "normal" | "parent_task" | "subtask";

interface Task {            // existing, plus:
  kind: TaskKind;           // absent in old daemons → treat as "normal"
  parent_task?: string;     // subtasks only
  subtask_progress?: Progress; // parent tasks only (computed)
}

interface Progress { merged: number; counted: number; dropped: number; ready: boolean }

interface SubtaskSummary {
  key: string; title: string; status: TaskStatus;
  archived: boolean; dropped: boolean;
  pr: { url: string; state: "open" | "draft" | "merged" | "closed" } | null;
}

interface ProjectSettings { /* existing */ parent_task_merge: "merge" | "squash" | "rebase" }
```

## Endpoints

| Method | Path | Change |
|---|---|---|
| POST | `/api/v1/tasks` | body gains `kind?: TaskKind`, `parent_task?: string` |
| GET | `/api/v1/tasks`, `/api/v1/tasks/{key}` | items gain `kind`, `parent_task`, `subtask_progress` |
| GET | `/api/v1/tasks/{key}/subtasks` | **new** → `SubtaskSummary[]` (404 if not a parent task) |
| POST | `/api/v1/tasks/{key}/make-parent-task` | **new** → `Task` |
| POST | `/api/v1/tasks/{key}/attach` | **new**, body `{ parent_task }` → `Task` |
| POST | `/api/v1/tasks/{key}/prs/{id}/ready` | **new** → PR DTO; runs `gh pr ready` |
| POST | `/api/v1/tasks/{key}/prs` (open) | body gains `empty_commit?: bool` |
| POST | `/api/v1/tasks/{key}/prs/{id}/merge` | body gains `confirm_live_subtasks?: bool` |
| GET | PR DTO | gains `method?: "merge" \| "squash" \| "rebase"` (what Merge will use) |
| GET | `/api/v1/tasks/{key}/base-status` | gains `subtask_commits: number` (parent tasks) and `base: string` (branch compared against) |
| POST | `/api/v1/runs` (start) | refusals below |
| POST | `/api/v1/tasks/{key}/update-branch` | merges the refs listed in research R9, in order |
| POST/DELETE | `/api/v1/tasks/{key}/archive`, `/api/v1/tasks/{key}` | refusal `live-subtasks` |

## Refusals (409, `{ "error": <reason>, "message": <localised>, ...detail }`)

| Reason | When | Detail |
|---|---|---|
| `not-a-parent-task` | `parent_task` names a normal task or a subtask | `key` |
| `parent-task-archived` | adding/attaching to an archived parent task | `key` |
| `nesting` | a parent task given a `parent_task`, or making a subtask a parent task | — |
| `project-mismatch` | subtask with an explicit project ≠ parent task's | `project` |
| `parent-task-not-started` | starting a subtask whose parent task has no workspace | `key` |
| `parent-task-branch-missing` | `origin/<branch>` not found after fetch | `branch` |
| `has-open-pr` / `has-session` / `in-checkout` | make-parent-task preconditions (FR-008) | — |
| `already-started` | attach of a task that has run | — |
| `live-subtasks` | archive/delete of a parent task; merge without confirmation | `subtasks: string[]` |
| `no-commits` | open PR with nothing to propose (existing); parent task may retry with `empty_commit` | — |
| `empty-commit-not-allowed` | `empty_commit` on anything but a parent task | — |

## CLI

| Command | Effect |
|---|---|
| `delilah new --parent-task "<title>" [--project P]` | creates a parent task |
| `delilah new --parent <KEY> "<title>"` | creates a subtask of KEY (project from KEY) |
| `delilah make-parent-task <KEY>` | converts a normal task |
| `delilah attach <KEY> --parent <PARENT>` | attaches a never-started normal task |
| `delilah ls` | subtasks indented under their parent task; parent task shows `N/M` |
| `delilah show <KEY>` | adds `Kind`, `Parent task`, `Progress` and the subtask list |

`--parent-task` and `--parent` are mutually exclusive. Refusals print the localised message
and exit 1.
