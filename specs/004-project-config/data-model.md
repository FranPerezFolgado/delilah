# Data model: Per-project configuration

## ProjectSettings (`delilah-core::project`, stored as JSON in `projects.settings`)

| Field | Type | Default | Rules |
|---|---|---|---|
| `workspace` | `"checkout" \| "worktree"` | `"checkout"` | |
| `branch` | string | `"feature/{ticket}"` | must contain `{ticket}`; only `{ticket}` and `{slug}` as placeholders; non-empty |
| `shared` | list of `SharedEntry` | `[]` | paths unique |
| `setup` | string or null | null | trimmed; empty means none |
| `submodules.always` | list of submodule paths | `[]` | parent projects only; each must exist in the parent |
| `submodules.never` | list of submodule paths | `[]` | parent only; must exist; no path in both lists |

Every field has `#[serde(default)]`, so `'{}'` (all existing rows after the migration)
deserialises to the defaults, which are today's behaviour (FR-002). The API always
returns all fields (FR-017). Unknown keys are kept out by `deny_unknown_fields` on
input; `pr` is added later with the PR work (FR-014).

## SharedEntry

| Field | Type | Default | Rules |
|---|---|---|---|
| `path` | string | | relative; no `..` component; not absolute; not empty |
| `mode` | `"copy" \| "link"` or null | null → by kind at use: folder `link`, file `copy` | |

## Validation errors

`validate()` returns a list of `{ field, message }`, where `field` names the input
(`branch`, `shared[2].path`, `submodules.never[0]`…), so the screen can show each one
next to its field (FR-004). Messages are localised by the daemon (`Lang::pick`).

## TaskStatus additions (`delilah-core::task`)

| Status | From | Event | To |
|---|---|---|---|
| `SetupFailed { reason }` (`setup-failed`) | `PreparingWorkspace` | `SetupFailed { reason }` | |
| | `SetupFailed` | `RetrySetup` | `PreparingWorkspace` |
| | `SetupFailed` | `SkipSetup` | `PreparingWorkspace` (then `WorkspaceReady` → `Running`) |
| | `SetupFailed` | `Fail { reason }`, `MarkDone` | `Failed`, `Done` |

`SetupFailed` is not terminal. `reason` holds the last 20 lines of output or
"cancelled" / "timed out" / "interrupted".

## Session kind addition (`store_sessions::SessionKind`)

`Setup`: one per setup run, in the task's worktree, read-only for the user (no input
forwarded), closed when the command ends. Its log file is the setup's full output.

## Migration `0011_project_settings.sql`

```sql
-- What the user set for the project in Delilah (JSON); '{}' = every default.
ALTER TABLE projects ADD COLUMN settings TEXT NOT NULL DEFAULT '{}';
```
