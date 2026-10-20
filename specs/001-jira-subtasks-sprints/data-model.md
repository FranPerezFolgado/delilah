# Data model

## `JiraIssue` (crates/delilah-integrations/src/jira.rs) — new fields

| Field | Type | Notes |
|---|---|---|
| `parent` | `Option<JiraParent>` | Only when the `parent` is **not** an epic (sub-task → story/task). `#[serde(default)]`. |
| `subtask` | `bool` | `issuetype.subtask == true` or `hierarchyLevel == -1`. `#[serde(default)]`. |
| `sprint` | `Option<JiraSprint>` | The relevant sprint (active > future > last). `#[serde(default)]`. |

`epic` stays the same, but for a sub-task with no epic of its own it is **inherited** from its parent.

## `JiraParent`

| Field | Type |
|---|---|
| `key` | `String` |
| `summary` | `Option<String>` |

## `JiraSprint`

| Field | Type | Notes |
|---|---|---|
| `name` | `String` | |
| `state` | `String` | `active` \| `future` \| `closed` (lower case, normalised from DC's `ACTIVE`). |

## `Filter`

| Field | Type | Default |
|---|---|---|
| `who` | `Who` | `assigned` |
| `progress` | `Progress` | `open` |
| `sprint` | `SprintFilter` (`all` \| `current`) | `all` — new |

## Rules

- A row with `parent` set whose parent is in the list → shown nested under it.
- A row with `parent` set whose parent is not in the list → top level, with "↳ parent.key parent.summary".
- Group counts = top-level rows. Parent row: badge with its number of sub-tasks.
- Text filter: a parent is shown if it or any of its sub-tasks matches; if the parent matches, all of its sub-tasks are shown.
