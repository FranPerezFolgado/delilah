# Phase 7 contract review (WP-17, 8d80308)

Contracts: POST /api/v1/tasks/{key}/make-parent-task and POST /api/v1/tasks/{key}/attach (body `{parent_task}`), both returning Task.

Producers: crates/delilah-daemon/src/api_hierarchy.rs (make_parent_task, attach), routes in api.rs.
Consumers: app/src/api.ts:873,875 (makeParentTask, attach; added in an earlier commit, body field `parent_task` matches AttachBody), crates/delilah-cli/src/main.rs (Attach --parent, MakeParentTask), BacklogTaskView.tsx, TaskHeader.tsx.

[OK] attach request/response: UI and CLI send `{parent_task}`; the daemon reads `parent_task: String`. Both return Task.
[OK] make-parent-task: no body; the UI and CLI send none or `{}`, which the handler ignores.
[OK] Refusal reasons used (nesting, in-checkout, has-session, has-open-pr, not-a-parent-task, parent-task-archived, already-started, project-mismatch) are all in contracts/api.md; HTTP 409 with `{error, message, key}` via ApiError::refused. Tests cover each.
[OK] CLI `attach <KEY>` without --parent still goes to the terminal attach, so the existing command is unchanged. `attach KEY --parent P` matches contracts/api.md. `make-parent-task` matches.

[RISK/LOW] attach with an unknown parent key returns 409 `not-a-parent-task` and not 404. This agrees with the contract text ("names a normal task or a subtask") but is not documented for a missing key. Fix: add "or unknown" to the contracts/api.md row.
[RISK/LOW] UI AttachDialog (BacklogTaskView.tsx) hides parents with status done/failed. The daemon only refuses a terminal parent when it has no workspace (archived), so a done parent that is not archived is attachable via the API/CLI but not in the UI. This is a UI-only narrowing, not a break.
[RISK/LOW] The `attach` handler's checks, `UPDATE project` and `set_kind` are not in one transaction. A concurrent start between the check and the write could attach a started task. The same applies to make_parent_task, whose `UPDATE` and `set_kind` are two statements. Fix: wrap in a transaction, or accept the single-user race.
[RISK/LOW] `make_parent_task` on a task that is already a parent returns 200 with no change (idempotent). contracts/api.md does not say so; add a line.

No CRITICAL or HIGH findings.

VERDICT: COMPATIBLE
