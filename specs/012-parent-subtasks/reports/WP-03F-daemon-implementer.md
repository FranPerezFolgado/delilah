## WP-03F-daemon — done (HIGH only)
Tasks:    HIGH ✓; MEDIUMs skipped (not cheap without risk: run.rs parent-start extraction, StartPlan fields)
Files:    crates/delilah-daemon/src/integration.rs — new push_workspace_branch (iterates ws.repos())
          src/run.rs, src/api_hierarchy.rs — both call it
          tests/parent_tasks.rs — submodule branch deleted on remote, push-branch repairs it
Tests:    extended a_subtask_takes_the_parent_task_submodules_in_a_parent_project; cargo test -p delilah-daemon, cargo check --workspace pass
Notes:    StartPlan.in_checkout/branchless and run.rs inline branch left as is.
