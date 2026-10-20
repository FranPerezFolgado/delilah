# Data model: Agent playground

Nothing is stored and there is no migration. Everything below lives in the daemon's
memory, on each live session, and is gone when the session ends.

## Activity (per live Claude session)

`Live.activity: Arc<Mutex<Activity>>` in `crates/delilah-daemon/src/sessions.rs`.

| Field | Type | Notes |
|---|---|---|
| `action` | `Option<Action>` | the main agent's current action |
| `helpers` | `Vec<Helper>` | in start order; a helper's index never changes while it runs |

## Action

| Field | Type | Notes |
|---|---|---|
| `id` | `String` | `tool_use_id`; a `Post…` only clears an action with the same id |
| `kind` | `ActionKind` | `edit` \| `read` \| `run` \| `delegate` \| `other` |
| `tool` | `String` | the tool's name, e.g. `Edit`, `mcp__jira__search` |
| `target` | `Option<String>` | ≤ 40 characters, already cut with "…" |
| `background` | `bool` | `tool_input.run_in_background == true` |
| `since` | timestamp | |

`supervising` (in the view) = `kind == delegate && !background`.

## Helper

| Field | Type | Notes |
|---|---|---|
| `id` | `String` | `agent_id` |
| `role` | `String` | `agent_type`, e.g. `code-researcher`, `implementer`, `Explore` |
| `action` | `Option<Action>` | its own current action |
| `since` | timestamp | |

## Transitions

| Hook | Without `agent_id` (main agent) | With `agent_id` (helper) |
|---|---|---|
| `PreToolUse` | `action = new` | upsert the helper; `helper.action = new` |
| `PostToolUse`, `PostToolUseFailure` | clear `action` if the id matches | upsert the helper; clear its action if the id matches |
| `SubagentStart` | n/a | upsert the helper |
| `SubagentStop` | n/a | remove the helper |
| `Stop`, `StopFailure`, `SessionEnd` | clear `action` and all helpers | (a helper's own `Stop` does not happen; `SubagentStop` is used) |
| state → `Waiting` / `Paused` | clear `action` and all helpers | |

Every change that alters `Activity` emits `Topic::Activity`. A Pre/Post pair that leaves
it equal emits nothing.

## UI-side

- **Slot map** (`assignSlots`): `Map<project, Map<sessionId, slotIndex>>`, kept in a ref
  inside the view. It is not stored.
- **Desk** (derived): session, task (key, title, project, phase via `columnOf`), state,
  activity.
- **Role look** (`roleLook(role)`):

  | Role | Look |
  |---|---|
  | `code-researcher`, `Explore`, `doc-searcher` | researcher (magnifying glass) |
  | `implementer` | implementer (hard hat) |
  | `*-reviewer`, `spec-compliance-reviewer` | reviewer (glasses) |
  | anything else | generic, with the role name as label |
