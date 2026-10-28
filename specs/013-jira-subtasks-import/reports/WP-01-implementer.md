## WP-01 — done
Tasks:    T001 ✓, T002 ✓
Files:    crates/delilah-daemon/src/jira_import.rs — new, doc comment only
          crates/delilah-daemon/src/lib.rs — `mod jira_import;`
          crates/delilah-daemon/src/store.rs — `mod jira_skips;` (child module, file in src/store/)
          crates/delilah-daemon/src/store/jira_skips.rs — new, empty `impl Store {}`
          crates/delilah-daemon/migrations/0026_jira_subtask_skips.sql — table, PK (parent_task, jira_key), no FK
Tests:    none added (setup); `cargo test -p delilah-daemon` passes (1092 + 7), no warnings
Notes:    none
