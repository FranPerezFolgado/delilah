# Phase 3 security review (WP-07, d14a1c0)

Verdict: no CRITICAL or HIGH findings.

Checked:
- Path key: check_key / is_issue_key on GET and POST; JQL only gets validated keys (integrations subtasks / my_subtask_keys re-check).
- Jira-supplied subtask keys: not checked with is_issue_key in the import, but every new task goes through resolve_new_task -> TaskKey::parse, so a key like "../x" is refused before it reaches workspaces_dir().join(key). Safe.
- add/skip/unskip must be keys of the live Jira sub-task list (not-a-subtask 400), so arbitrary keys cannot enter jira_skips or touch unrelated tasks.
- project is trimmed and looked up with store.project (parameterised); all SQL in live()/attach/skips is bound parameters.
- Races: preview happens outside the transaction, but apply uses BEGIN IMMEDIATE and re-reads each task with live(), re-running check_attach; stale parent returns 409; taken subtasks go to left_out. Sound.

[LOW] crates/delilah-daemon/src/jira_import.rs (apply, live)
Problem: one Jira sub-task with a key that TaskKey::parse rejects fails the whole import with 400 after preview (availability only).
Fix: filter rows whose key fails TaskKey::parse into left_out ("bad-key") before building tasks.

[LOW] crates/delilah-daemon/src/jira.rs write_ticket (Parent line)
Problem: parent summary is written raw into ticket.md like the Epic line; Jira text is untrusted prompt content (prompt injection is the same class as the existing description/epic fields; newlines are not possible in Jira summaries).
Fix: optional: collapse whitespace/newlines in summary before writing.

[LOW] jira_import.rs live()/apply
Problem: ApiError::internal(e.to_string()) returns raw sqlx error text to the client (same pattern as the rest of the daemon; local-only API).
Fix: log the error, return a generic message.
