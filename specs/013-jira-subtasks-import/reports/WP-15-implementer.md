## WP-15 — done
Tasks:    T040 ✓, T041 ✓, T042 ✓
Files:    app/src/components/SubtaskList.tsx — Jira status badge for linked rows, "not in Jira any more" badge for gone keys
          app/src/components/SubtaskList.test.tsx — new test (linked / gone / not linked)
          app/src/components/SubtaskList.css — styles for .subtasks-banner, .subtasks-note, .subtasks-jira
          app/src/i18n/en/SubtaskList.ts — "ya no está en Jira"
          crates/delilah-daemon/src/jira_import.rs — test only
Tests:    SubtaskList test "shows the Jira status of linked subtasks…"; daemon a_subtask_moved_to_another_ticket_in_jira_is_gone_and_left_alone. tsc + vitest SubtaskList/i18n pass; cargo test -p delilah-daemon jira_import pass (26).
Notes:    No daemon code change needed (preview already supplies status and gone). api.ts untouched.
