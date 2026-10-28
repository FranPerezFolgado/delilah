## WP-02 — done
Tasks:    T003 ✓, T004 ✓
Files:    crates/delilah-integrations/src/jira.rs — JiraClient::my_subtask_keys (Sample filter / Remote JQL `parent = KEY AND assignee = currentUser()`, fields=key, paged Cloud+DC); new test; updated PROJ-1246 count assertion (1,1)->(4,1)
          crates/delilah-integrations/fixtures/jira-search.json — PROJ-1255 (open, unassigned), PROJ-1256 (open, Fran), PROJ-1257 (open, Ana) added under PROJ-1246 (which already has PROJ-1252 done, Ana)
Tests:    my_subtask_keys_are_only_those_assigned_to_me; `cargo test -p delilah-integrations` passes (20)
Notes:    Fixture ticket is PROJ-1246 (not PROJ-1242, to avoid breaking its key/count tests).
          OUT OF SCOPE / follow-up: crates/delilah-e2e/tests/ui_v2.rs:~648 asserts PROJ-1246 counts (Some(1), Some(1)); now (Some(4), Some(1)). Needs updating by whoever owns that file. Other crates' sample-based tests (daemon jira_import, listings of "my" tickets) may also see the new sub-tasks (PROJ-1256 is Fran's).
Update: edited crates/delilah-e2e/tests/ui_v2.rs:648 to (Some(4), Some(1)). cargo test -p delilah-daemon does not compile now (E0603 module `task` private, other implementers files), so daemon sample tests are unverified against the new sub-tasks.
