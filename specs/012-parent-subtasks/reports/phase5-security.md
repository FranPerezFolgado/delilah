# Phase 5 security review (f2bec6a..HEAD)

Verdict: no CRITICAL or HIGH issues.

Checked:
- ready endpoint: PR row is scoped to the task key, URL comes from the DB, must start with https://, passed after `--`; only on explicit POST. OK.
- empty commit: message passed as a single argv element (`-m`), no shell; refused for non-parent tasks. OK.
- merge confirmation: `confirm_live_subtasks` is a client-supplied flag on a local API, a UX guard and not a security boundary. OK.
- base_update merge_refs: ref names validated (valid_ref_name), `--` before ref. OK.
- pr_loop notices: static text, dedupe by PR URL. OK.

[LOW] crates/delilah-daemon/src/pr_api/ready.rs:28
Problem: no explicit gh account check before `gh pr ready`, unlike what the brief expects; it relies on the ambient gh login, same as pr_merge.
Fix: if multi-account safety is required, verify the active gh user matches the PR host/owner before calling.

[LOW] crates/delilah-daemon/src/pr_api/open.rs (open_with)
Problem: task.title is embedded in the commit message unsanitised (newlines allowed); harmless, only affects the message.
Fix: optionally take the first line only.
