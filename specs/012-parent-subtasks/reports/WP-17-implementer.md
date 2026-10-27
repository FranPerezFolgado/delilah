## WP-17 — done
Tasks:    T048 ✓, T049 ✓, T050 ✓, T051 ✓
Files:
- crates/delilah-daemon/src/api_hierarchy.rs — `make_parent_task` and `attach` handlers, plus 3 tests
- crates/delilah-daemon/src/api.rs — the two routes
- crates/delilah-cli/src/main.rs — `make-parent-task <KEY>`; `attach <KEY> --parent <P>`
- app/src/components/BacklogTaskView.tsx (+test) — "Hacer tarea padre" and "Adjuntar a tarea padre…" (picker dialog)
- app/src/components/TaskHeader.tsx (+test) — "Tarea padre" menu with "Convertir en tarea padre" on started normal tasks
- app/src/i18n/en/BacklogTaskView.ts, TaskHeader.ts — English texts
Tests:    api_hierarchy::tests::{make_parent_task_converts_a_normal_task_and_refuses_the_rest, a_running_task_converted_keeps_its_workspace_and_starts_no_agent, attach_makes_a_never_started_task_a_subtask_in_the_parents_project}; vitest for TaskHeader and BacklogTaskView. Passing: cargo test -p delilah-daemon (1092), cargo test -p delilah-cli (45), clippy -D warnings, tsc --noEmit, vitest on the two components + i18n.
Notes:
- `attach` already existed as the terminal command. It now takes an optional `--parent`: with it the command attaches to a parent task; without it, the terminal behaviour is unchanged.
- Attach refuses a project mismatch (`project-mismatch`, from `check_attach`). The task only takes the parent's project when it has none. The acceptance text ("project change") is covered by the no-project case.
- make-parent-task on a task that is already a parent task returns 200 and changes nothing. Open PR means a PR with no snapshot yet or an open one. Session means an open row in `sessions`. Started (for attach) means status other than queued, or a workspace.
- The node_modules symlink was removed.
