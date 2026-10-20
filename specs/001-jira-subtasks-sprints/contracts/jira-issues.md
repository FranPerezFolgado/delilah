# Contract: `GET /api/v1/jira/issues`

## Query

| Param | Values | Default | Status |
|---|---|---|---|
| `who` | `assigned` \| `reported` \| `query` | `assigned` | existing |
| `progress` | `open` \| `done` | `open` | existing |
| `sprint` | `all` \| `current` | `all` | **new** |

Unknown params are still ignored; an invalid value → 400 (axum `Query`).

## Response `200` — `IssueRow[]` (flat)

Existing fields unchanged. New ones per row:

```json
{
  "key": "PROJ-1250",
  "subtask": true,
  "parent": { "key": "PROJ-1242", "summary": "Rechazar cobros duplicados en una ventana de 30 s" },
  "epic": { "key": "PROJ-1200", "summary": "Pagos sin sobresaltos" },
  "sprint": { "name": "Sprint 42", "state": "active" },
  "assignee": "Ana",
  "task_status": null
}
```

- Order: top-level tickets in the JQL order, then sub-tasks (the UI nests them).
- `parent` is `null` for top-level tickets; `sprint` is `null` in the backlog or with no Jira Software.

## Errors

- `422` with Jira's message if the site rejects `sprint` (no Jira Software).

## Frontend (`app/src/api.ts`)

```ts
export type JiraSprintFilter = "all" | "current";
jiraIssues: (who, progress, sprint: JiraSprintFilter) =>
  request<JiraIssue[]>(`/api/v1/jira/issues?who=${who}&progress=${progress}&sprint=${sprint}`)
// JiraIssue += subtask: boolean; parent: { key: string; summary: string | null } | null;
//              sprint: { name: string; state: string } | null;
```
