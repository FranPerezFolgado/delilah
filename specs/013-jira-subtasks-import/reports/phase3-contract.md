# Phase 3 contract review: Jira import GET/POST /api/v1/jira/issues/{key}/import

Producer: crates/delilah-daemon/src/jira_import.rs (get_preview, post_import), route in api.rs:163.
Consumer: app/src/api.ts:374-425, 946-949. Spec: specs/013-jira-subtasks-import/contracts/api.md.

[BREAKING] JiraImportTask.status type: jira_import.rs:56-62 <-> api.ts:378
Problem: daemon serializes `ExistingTask.status` as `TaskStatus` (serde `tag="type"`, delilah-core task.rs:50), i.e. an object `{"type":"running"}` / `{"type":"failed","reason":"..."}`. Client declares `status: string`, and the spec example is also loose. Any `row.task.status` use as a string renders "[object Object]" or fails comparisons.
Fix: type it as the existing `TaskStatus` union in api.ts (line 7), and use `statusLabel`/`isActive`; or have the daemon send `status.name()`. Update contracts/api.md accordingly.

[RISK] Error body shape: jira_import.rs:299-307 <-> contracts/api.md:4 <-> api.ts:904-912
Problem: spec says errors use `{ error, code? }`; the daemon sends refusal-style `{ error: <code>, message }` (400 bad-key / project-required / not-a-subtask, 409 refusal code). The client `request()` copes (message -> ApiError.message, code -> ApiError.reason, body in `detail`), so it works, but callers must read `err.reason` for the code, and the spec text is wrong.
Fix: correct the spec to `{ error: <code>, message }` and note that the code lands in `ApiError.reason`.

[RISK] Undocumented POST 409 for unlinked Jira: jira_import.rs:843-848
Problem: POST when Jira is not-connected / not-found / unreachable returns 409 `{ error: "Jira can't tell about KEY: <reason>", reason }` (not a coded body; `reason` is not-connected, not-found or unreachable). Not in the contract. The client gives ApiError with status 409 and reason, so it is handled, but a 409 here is ambiguous with a refused parent (409 with code in reason).
Fix: document it, or return a coded body, so the UI can tell apart "refused" from "unlinked".

[MEDIUM] GET `?project=` query: jira_import.rs:481-495 <-> api.ts:946
Problem: the daemon accepts an optional `project` query (it affects the preview), but the spec omits it and `jiraImportPreview(key)` cannot pass it.
Fix: add an optional `project` parameter to the client and the spec, or drop it from the daemon.

[MEDIUM] Loose enums in the client: api.ts:383-395, 399-401
Problem: `ParentAction`: create/convert/existing/refused{code,message} matches the daemon exactly. `JiraImportRow.action` + `reason` match, but only conflict rows have `reason`; for other actions the flattened enum omits the key, so `reason` is `undefined`, not `null` (the spec example shows `"reason": null`, and the client says `string | null`). `code`/`left_out.reason`/`task.kind` are plain strings (kind is `normal|parent_task|subtask`).
Fix: type `reason?: string`; optionally union the known codes.

[OK] Matches: ticket {key,title,status,is_subtask}; subtasks fields key/title/status/status_category/assignee/mine/task/skipped/selected; gone {key,title}; linked:false reasons (not-connected|not-found|unreachable); POST request (project optional, add/skip/unskip default []) and result (parent_task, created, attached, left_out[{key,reason}], skipped, unskipped); status codes 400 bad-key / project-required / not-a-subtask, 409 refusal. `linked` is flattened via untagged enum; plan fields sit at top level as the client expects. Request field `project?` is fine (daemon accepts null/absent).

[OK] No tests for round trip of the TS types against daemon JSON (the daemon test at jira_import.rs:1250 checks only `linked`). Consider a JSON fixture shared by both.

VERDICT: ❌ BREAKING (one type mismatch, JiraImportTask.status; the rest are RISK or MEDIUM)
