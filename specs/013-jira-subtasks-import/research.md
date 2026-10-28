# Research: Jira sub-tasks become Delilah subtasks

Each entry: decision, rationale, alternatives ruled out. Code references are to `main` at
`a1a11fb`.

## R1. One preview endpoint for the Jira view and the parent task screen

**Decision**: `GET /api/v1/jira/issues/{key}/import` returns the ticket, what the Delilah task
with that key is (none / normal and convertible / parent task / refused, with the reason) and
every Jira sub-task with its proposed action and default selection. The Jira view's dialog and
the parent task screen's "N new Jira sub-tasks" offer both read it; the parent task screen
also takes the Jira status of each existing subtask (US4) and the "not in Jira any more" ones
from it.

**Rationale**: Both screens need the same join (Jira sub-tasks × Delilah tasks × skips), and
computing it in the daemon keeps the rules (FR-003, FR-007, FR-008) in one place, testable
without the UI.

**Alternatives**: computing the join in the UI from `jiraSubtasks` + `tasks` (rules duplicated
in TS, and the CLI would need them too); a separate `GET /tasks/{key}/jira-subtasks` (same
data, two routes to keep in sync).

## R2. One write endpoint, one transaction

**Decision**: `POST /api/v1/jira/issues/{key}/import` with `{ project?, add, skip, unskip }`.
It re-reads the preview (never trusts the client's view of Jira), then in one
`BEGIN IMMEDIATE` transaction: inserts the parent task or turns the existing task into one,
inserts or attaches the subtasks in `add`, and writes/removes skips. Sub-tasks in `add` that
are not eligible are left out and reported (FR-008), not an error; a parent task that cannot
be made is a 409 with spec 012's refusal codes and nothing is written (FR-006, FR-007).

**Rationale**: FR-006 asks for all or nothing. `Store::create_task_from_conversation`
(`store.rs:86`) already shows the transaction pattern; `insert_task` takes any executor
(`store.rs:639`), and `set_kind` (`store.rs:506`) is one `UPDATE` that gets an executor-taking
twin.

Facts that can change between the preview and the write (the ticket task's kind, open PR or
live session; an attach target being started) are re-checked inside the transaction; Jira is
read outside it. The convert reuses one store function with `make_parent_task`
(`become_parent_task`: clears workflow, autopilot, CI autofix, saving mode, handoff, sets the
kind), and new tasks come from a pure constructor split out of `build_task_with`.

**Alternatives**: calling the existing `POST /tasks`, `make-parent-task` and `attach` routes
one by one from the UI (no atomicity, partial state on failure); a batch endpoint on
`/tasks` (would bake Jira knowledge into the generic route).

## R3. Rules reused from `delilah_core::hierarchy`

**Decision**: The import decides each action with spec 012's checks: `check_make_parent_task`
(`hierarchy.rs:96`) for an existing ticket task, `check_new_subtask` (`hierarchy.rs:81`) for
new subtasks, `check_attach` (`hierarchy.rs:119`) for existing sub-task tasks. A pure function
`plan_import(ticket, existing, subtasks, tasks, skips, me) -> ImportPlan` in a new daemon
module holds the decision table so it is unit-tested without Jira or SQLite.

**Rationale**: No second copy of the hierarchy rules; the refusal codes and messages stay the
ones the UI already translates.

## R4. "Assigned to me" from Jira itself

**Decision**: The client gets `my_subtask_keys(parent) -> HashSet<String>`: for Jira, one JQL
search `parent = KEY AND assignee = currentUser()` (keys only); for the sample data, the
sub-tasks whose assignee is `SAMPLE_ME` (`jira.rs:169`). A failure of that search degrades to
"nobody is me" (only unassigned open sub-tasks preselected), never to an error.

**Rationale**: `JiraIssue.assignee` is a display name (`jira.rs:669`), which can collide or
change; `currentUser()` is exact and is already how the list filters "assigned"
(`jira.rs:143`). The key in the JQL is checked with `is_issue_key` as `subtasks()` does
(`jira.rs:507`).

**Alternatives**: comparing display names with `myself()` (ambiguous); adding `accountId` to
`JiraIssue` (a bigger change to a shared struct, and Data Center uses `name` instead).

## R5. Skips are a small table

**Decision**: Queries in `store/jira_skips.rs`. Migration `0026_jira_subtask_skips.sql`: `jira_subtask_skips(parent_task TEXT
NOT NULL, jira_key TEXT NOT NULL, created_at TEXT NOT NULL, PRIMARY KEY (parent_task,
jira_key))`. Deleting a parent task deletes its skips in the same place that calls
`clear_parent_task_links` (`actions.rs`). Archiving does not.

**Rationale**: FR-011 needs skips to persist per parent task; a table with a composite key is
idempotent (`INSERT OR IGNORE`) and needs no JSON parsing. A reused key never inherits old
skips.

**Alternatives**: a JSON column on `tasks` (a migration on the hot table for a rarely used
list); `projects.settings` (wrong owner).

## R6. Sync is pull on open, no background job

**Decision**: The parent task screen calls the preview when it mounts (and on a "Check Jira"
button); the request runs in the background of the UI, so the screen renders first. The CLI
has `delilah jira import` for the same thing. No poller, no `notices`.

**Rationale**: FR-012 rules out background polling; the Jira HTTP client already has a 20 s
timeout (`jira.rs:270`), and nothing else on the screen waits for it (SC-005).

**Alternatives**: a periodic poll in the PR loop (ruled out by the spec: load on Jira and the
token for no user action).

## R7. A parent task is "linked to Jira" when Jira knows its key

**Decision**: There is no stored link: a parent task is linked when its key is an issue key
and Jira returns it. Hand-made parent tasks with a Jira key get the offer too (US3-5); `LIBRE-n`
keys just get a quiet "not a Jira ticket" (404 from Jira → `linked: false`, no error toast).

**Rationale**: Keys already are Jira keys (FR-009 holds by construction: subtasks are keyed by
the Jira sub-task); a stored flag would add a column with no new information.

## R8. Submodules: already inherited

**Decision**: No work. Spec 012 R7 already starts a subtask with the parent task's submodules
(`integration.rs:81–90`), and backlog tasks don't hold submodules; the parent task picks them
when it starts. FR-005 is covered by a test, not code.

## R9. The agent learns the parent ticket

**Decision**: `write_ticket` (`daemon/src/jira.rs:218`) adds a line `Parent: KEY title` when
`issue.parent` is set. Jira sub-tasks already carry `parent` (`integrations/src/jira.rs:61`).

**Rationale**: FR-015 with one line in the existing ticket file; `fetch_issue` at start
(`run.rs:931`) already writes the sub-task's own ticket.

## R10. Entry points

**Decision**: UI: "Create parent task" on a Jira ticket with `subtask_count > 0` that is not a
sub-task (JiraView), opening `JiraImportDialog`; on a parent task, `SubtaskList` shows the offer
and opens the same dialog in "add" mode, with each subtask's Jira status and the skipped list.
CLI: `delilah jira import KEY [--project P] [--add K,..|--defaults] [--skip K,..]
[--unskip K,..]`; with none of them it prints the preview (the dry run).
API as R1/R2.

**Rationale**: Spec 012 offers every hierarchy action from UI, CLI and API; one CLI command
covers both first import and later sync since the endpoint is the same.
