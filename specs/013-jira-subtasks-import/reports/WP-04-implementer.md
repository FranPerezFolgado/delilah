## WP-04 — done
Tasks:    T008 ✓, T009 ✓, T010 ✓
Files:    crates/delilah-daemon/src/store/jira_skips.rs — add_jira_skips/remove_jira_skips take `conn: &mut SqliteConnection` and `&HashSet<String>` (INSERT OR IGNORE); Store::jira_skips_of (sorted Vec<String>); Store::delete_jira_skips; 4 tests
          crates/delilah-daemon/src/actions.rs — delete_jira_skips after clear_parent_task_links (ParentTask only); tests deleting_a_parent_task_drops_its_jira_skips, archiving_a_parent_task_keeps_its_jira_skips
          crates/delilah-daemon/src/store.rs — one line: `pub(crate) use jira_skips::{add_jira_skips, remove_jira_skips};`
Tests:    cargo test -p delilah-daemon jira_skips: 6 passed
Notes:    dead-code warnings for the three fns until WP-06 uses them (no #[allow] added).
