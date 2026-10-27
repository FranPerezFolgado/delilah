## WP-04 — done
Tasks:    T006 ✓, T007 ✓
Files:    crates/delilah-daemon/migrations/0025_task_hierarchy.sql — new (kind, parent_task, index)
          crates/delilah-daemon/src/store.rs — kind/parent_task in insert_task, SELECTs, task_from_row; subtasks_of, set_kind, clear_parent_task_links; tests
Tests:    hierarchy_round_trips_and_subtasks_of_lists_children, migration_0025_makes_existing_tasks_normal; cargo test -p delilah-daemon (1047 pass), cargo check --workspace pass
Notes:    TaskKind imported via delilah_core::hierarchy::TaskKind (not re-exported at crate root). No other Task literals needed fixing. rustfmt ran on store.rs only.
