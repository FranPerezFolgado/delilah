## WP-17 — done
Tasks:    T039 ✓, T043 ✓
Files:    crates/delilah-daemon/src/phases.rs — on_hook nudges the PR loop on Signal::TurnEnded when store.prs_of(task) is non-empty; new test
Tests:    a_turn_end_nudges_the_pr_loop_only_for_a_task_with_prs; cargo test -p delilah-daemon -- phases hooks: 54 passed; cargo clippy -p delilah-daemon --all-targets: clean
Notes:    Test written together with the change (not observed failing first). Nudge happens for any primary/secondary session turn end, independent of the primary/limited flags.
