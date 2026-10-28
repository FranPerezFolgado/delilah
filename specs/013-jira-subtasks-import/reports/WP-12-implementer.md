## WP-12 — done
Tasks:    T033 ✓, T036 ✓ (already implemented earlier in apply_jira_import/post_import; verified, tests added)
Files:    crates/delilah-daemon/src/jira_import.rs — two new handler tests
Tests:    skips_and_unskips_need_a_parent_and_are_silent_writes, a_libre_parent_is_not_found_and_deleted_or_archived_subtasks_follow_the_store; cargo test -p delilah-daemon: 1133 passed
Notes:    skip/unskip validation (not-a-subtask), in-transaction add/remove_jira_skips and skip-only no-event already existed and pass; request fields skip/unskip match app/src/api.ts. No code change needed beyond tests.
