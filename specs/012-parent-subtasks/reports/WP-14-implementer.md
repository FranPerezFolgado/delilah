## WP-14 — done
Tasks:    T035 ✓, T036 ✓, T040 ✓, T041 ✓
Files:
- crates/delilah-daemon/src/base_update.rs — RepoState gains own ref + subtask_commits; BaseStatus.subtask_commits; update merges [origin/<own>, default] via delilah_git::merge_refs (one call, rollback inside + earlier repos reset); conflict message names first conflicting ref
- crates/delilah-daemon/src/pr_loop/merge.rs — on_subtask_merged (emits Topic::Tasks when changed, then parent_ready), parent_ready (notice KIND_PARENT_TASK_READY, cause "pr <url>", dedupe by insert_notice; needs ready progress + open draft parent PR)
- crates/delilah-daemon/src/pr_loop/mod.rs — run_check calls them (subtask all merged; parent task every check)
Tests:    base_update: parent counts subtask commits, merges own then default, refuses tracked changes, conflict in 2nd merge undoes first; pr_loop::merge: Tasks topic + ready once, not ready with open subtask/no draft, two merged in one tick. cargo test -p delilah-daemon pass (1085 + 7); cargo check --workspace pass.
Notes:    Topic carries no key, so "for the parent task" = Topic::Tasks. `merged` in update sums per-ref counts (may double count shared commits). cargo fmt -p ran on the crate; other implementer's files show as modified in git status (theirs).
