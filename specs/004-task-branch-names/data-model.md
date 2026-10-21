# Data model: Task branch names

No migration. Existing data is not changed.

## Task branch name (value, decided once at start)

`feature/` + (`<KEY>-` for a Jira task) + short name + (optional `-N`).

| Part | Rule |
|---|---|
| prefix | always `feature/`, exactly once |
| key | Jira tasks only, as is (`PROJ-12`); free tasks never carry `LIBRE-n` |
| short name | `[a-z0-9]+(-[a-z0-9]+)*`, at most 50 characters, cut at a word boundary |
| suffix | `-2` … `-99` (then a timestamp) when the name is taken in any repo of the workspace |

Where the short name comes from, in order: the name typed in the dialog or passed to the
API (`custom`, no key added) → Claude's proposal → slug of the title → the key
(`fallback`).

Stored in the existing `workspaces.branch` and read from there by the agent, PR loop,
archive and delete (unchanged).

## Task (API, `delilah_core::Task`)

New optional field, read-only:

| Field | Type | Notes |
|---|---|---|
| `branch` | `string \| null`, omitted when none | the stored `workspaces.branch`; none for backlog tasks and tasks without a workspace |

## Branch proposal (API response, not stored)

| Field | Type | Notes |
|---|---|---|
| `branch` | `string \| null` | full name, already free in the project's repos; `null` when only the key fallback is possible and the key isn't known yet (a new free task) |
| `source` | `"claude" \| "title" \| "key"` | where it came from |
