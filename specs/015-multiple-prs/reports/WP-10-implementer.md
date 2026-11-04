## WP-10 — done
Tasks:    T022 ✓, T027 ✓, T028 ✓
Files:
- crates/delilah-daemon/src/pr_loop/capture.rs — new: `is_pr_create(hook)`, `on_bash_output(app, key, hook)`, 8 tests
- crates/delilah-daemon/src/pr_loop/mod.rs — `pub(crate) mod capture;`; `link_known` now calls `link_pr_on_branch(key,&new,snapshot.head_branch)` (closed PR of same head branch is replaced, FR-009)
- crates/delilah-daemon/src/phases.rs — `on_hook` (Signal::Working) spawns `capture::on_bash_output` when `is_pr_create`; `nudge` kept
Tests:    capture::tests (stdout URL/foreign repo label, stderr "exists" + no duplicate, workspace repo ".", linked/unlinked skipped, other commands/tools/PreToolUse ignored, strict URLs only, unreadable URL reported not linked, command matcher). `cargo test -p delilah-daemon -- pr_loop phases hooks`: 110 pass; clippy clean.
Notes:
- Strictness (FR-004): hook_event_name must be PostToolUse, tool Bash, and a simple command (split at ; & | newline, after VAR= prefixes) must START with `gh pr create`; `echo gh pr create` does not count (stricter than signal_phase's `contains`). Words must be whole https PR URLs (parse_pr_url, then word == canonical url), max 4 per hook, deduped across stdout/stderr.
- Residual: `cat f; gh pr create` style chains mix outputs; any PR URL printed by another segment would be linked. Accepted.
- repo_of still returns NotFromTask for foreign repos (WP-12 owns that); capture maps NotFromTask to `host/owner/name` itself. WP-12 can drop that arm once repo_of does it.
- No test of the phases.rs wiring itself (spawned; covered by is_pr_create + on_bash_output tests).
- Failures use `failed(.., force=true)` with backoff key host/owner.
