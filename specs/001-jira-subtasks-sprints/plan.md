# Implementation Plan: Subtareas y sprint actual en la vista Jira

**Branch**: `feature/FEAT-2` | **Date**: 2026-10-20 | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `specs/001-jira-subtasks-sprints/spec.md`

## Summary

Two improvements to the Jira view: (1) bring in **every** sub-task of the
listed tickets, inherit the epic from their parent and show them **nested** under
it, in both groupings; (2) a **Sprint: Todos | Actual** filter (`sprint in openSprints() AND sprint not in futureSprints()`) and the name of the sprint on each ticket. The daemon keeps
returning a flat list with new fields (`parent`, `subtask`, `sprint`); the UI
builds the tree. See [research.md](research.md).

## Technical Context

**Language/Version**: Rust (edition 2024, `let` chains already in use) + TypeScript/React (Tauri app)

**Primary Dependencies**: reqwest, serde/serde_json, axum (daemon), React

**Storage**: N/A (nothing new is persisted; the sprint filter lives in the view's state, like the others)

**Testing**: `cargo test` (unit tests in `jira.rs` of both crates), `tsc --noEmit`

**Target Platform**: macOS (daemon + Tauri app)

**Project Type**: desktop-app + local daemon

**Performance Goals**: at most +2 calls to Jira per listing (sub-tasks; missing parents) and +1 on Cloud (`field`)

**Constraints**: Cloud (v3, `search/jql`) and Data Center (v2, `search`) on equal terms; sample data must cover every scenario

**Scale/Scope**: ≤ 50 top-level tickets + ≤ 100 sub-tasks per listing

## Constitution Check

`.specify/memory/constitution.md` is still the unfilled template; the
project's ground rules (`CLAUDE.md`) serve as gates instead:

| Gate | Status |
|---|---|
| Nothing from the work repos; Jira JSON fixtures only | ✅ fictional sample data in `fixtures/jira-search.json` |
| Hooks/settings outside the repo | ✅ not affected |
| No remote operations without confirming the account | ✅ no push/PR in this plan |
| MVP-0 scope (daemon + CLI + app, Claude adapter only) | ✅ improves an existing view |

Post-design re-check: no change.

## Project Structure

### Documentation (this feature)

```text
specs/001-jira-subtasks-sprints/
├── plan.md
├── research.md
├── data-model.md
├── quickstart.md
├── contracts/jira-issues.md
└── checklists/requirements.md
```

### Source Code

```text
crates/delilah-integrations/
├── src/jira.rs                 # JiraIssue/JiraParent/JiraSprint, SprintFilter, Filter::jql/matches,
│                               # custom_fields() (epic + sprint), map_issue/epic/parent/sprint,
│                               # list(): sub-task search, inherit_epics(), name_epics()
└── fixtures/jira-search.json   # sub-tasks, sprints (active/closed), backlog
crates/delilah-daemon/src/jira.rs   # doc of the `?sprint=` param, endpoint tests
app/src/api.ts                       # types + jiraIssues(who, progress, sprint)
app/src/components/JiraView.tsx      # sprint state + Segs, groupIssues → tree, child rows,
│                                    # ↳ hint, sprint tag, Padre/Sprint in the aside
app/src/styles.css                   # .jrow.child (indent), .jsubs (count badge)
app/src/i18n*                        # new strings ("Sprint", "Actual", "Todos", "Padre", "Sin sprint")
```

**Structure Decision**: the existing layout; no new modules.

## Design (by layer)

### 1. Integration (`delilah-integrations/src/jira.rs`)

1. **Types**: `JiraParent { key, summary: Option }`, `JiraSprint { name, state }`;
   `JiraIssue += parent, subtask, sprint` (all `#[serde(default)]`).
   `enum SprintFilter { #[default] All, Current }`; `Filter += sprint`.
2. **JQL**: `Filter::jql` builds the conditions as a list
   `[who?, progress, sprint?]` joined with `AND` + order. This also removes the
   `if who.is_empty()` branch.
3. **Custom fields**: `custom_fields() -> CustomFields { epic_link: Option, sprint: Option }`
   from a single `GET field`; `search` calls it on Cloud and DC and adds the ids to
   `fields`. `map_issue(issue, site, &CustomFields)`.
4. **Mapping**:
   - `epic()` unchanged.
   - `parent()`: `fields.parent` when it is not an epic → `JiraParent`.
   - `subtask`: `issuetype.subtask == true || issuetype.hierarchyLevel == -1`.
   - `sprint()`: array of objects or of DC strings → choose active > future > last.
5. **`list()`** (remote):
   1. `issues = search(filter.jql)`
   2. `keys` = non-sub-task tickets → if any, `search("parent in (…) AND issuetype in subTaskIssueTypes()")`
      with `maxResults=100`; append those not already listed.
   3. `inherit_epics()`: for sub-tasks with no epic, take the parent's from the list;
      for parents that are missing, `search("key in (…)")` → epic + summary of the parent.
   4. `name_epics()` (as today).
   `search` takes `max_results` as a parameter.
6. **Sample**: `list()` applies `matches` (now with sprint) to the non-sub-tasks,
   adds the sub-tasks whose parent passed, keeps the sub-tasks that pass
   on their own (orphans), and calls the same `inherit_epics` over the fixture.
   `CustomFields { sprint: Some("customfield_10020"), .. }`.

### 2. Daemon (`delilah-daemon/src/jira.rs`)

- `Query<Filter>` already deserialises `sprint`; just update the doc comment.
- Tests: `?sprint=current` with the sample, `?sprint=nope` → 400.

### 3. App

- `api.ts`: types and the `sprint` param.
- `JiraView.tsx`:
  - `const [sprint, setSprint] = useState<JiraSprintFilter>("all")`, in the `load` deps;
    `Segs label="Sprint"` in `.jbar` after "Estado".
  - `groupIssues` → first `nest(issues)`: `Map<parentKey, JiraIssue[]>`; top level =
    rows with no `parent` or whose parent is not in the list. The group is decided by the top level.
    Each `Group.issues` becomes `{ issue, children }[]`; the count = top level.
  - Text filter over the tree (parent shown if it or a child matches).
  - Child row: class `jrow child` (extra indent, no epic tag); parent row:
    `.jsubs` badge with `n` sub-tasks; orphan: `jtag` "↳ KEY".
  - Sprint tag on the row when `sprint` is `all` (with `current` it's redundant).
  - Aside: "Padre" rows (click → select it if it is in the list) and "Sprint".
- `styles.css`: `.jrow.child { padding-left: 58px }`, `.jsubs`.

## Testing strategy

See [quickstart.md](quickstart.md). Tests first in `jira.rs` (parsing,
JQL, sample `list`), then the daemon endpoint, then the UI by hand with sample data.

## Risks / limitations

- Searches page through results (Cloud `nextPageToken`, DC `startAt`); sub-tasks are capped at 500 per listing.
- The sub-task and missing-parent searches are best-effort: if Jira refuses them, a warning is logged and the tickets are still listed.
- `subTaskIssueTypes()` and `openSprints()` need Jira Software on DC; without it
  the sprint filter fails with a readable 422, and the sub-task search falls back
  to `parent in (…)` alone if Jira rejects the function (retry without it).
- Another `GET field` call on Cloud for each listing; can be cached in `JiraClient` if it gets noticeably slow.

## Complexity Tracking

No violations.
