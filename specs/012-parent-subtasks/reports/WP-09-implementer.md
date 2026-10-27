## WP-09 — done
Tasks:    T023 ✓, T024 ✓
Files:
- crates/delilah-daemon/src/api_hierarchy.rs — `subtasks` (GET, SubtaskSummary[], 404 for non-parent) and `push_branch` (POST, parent task only; 404 other kinds, 409 without workspace; calls delilah_git::push_branch directly, unconditional re-push) + 2 tests
- crates/delilah-daemon/src/api.rs — two routes
- crates/delilah-cli/src/main.rs — `new --parent-task` / `--parent KEY` (conflicting; `--project` now optional with --parent), `ls` nesting + `[N/M]`, `show` kind/parent/progress/subtask list, refusals print `message` (ErrorBody now reads it) + 5 tests
Tests:    cargo test -p delilah-daemon (1062 pass), cargo test -p delilah-cli (44 pass), cargo check --workspace OK
Notes:
- SubtaskSummary.pr.state is never "draft": integration::SubtaskPr carries no draft flag (integration.rs not editable); unread/open -> "open".
- CLI keeps `--title` (as existing `new`), not a positional title as in contracts/api.md.
- DELILAH_PROJECT env still fills --project; with --parent it would be sent and could trigger project-mismatch.
- No `make-parent-task` / `attach --parent` CLI (not in T024).
