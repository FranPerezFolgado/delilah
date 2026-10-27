## Spec compliance - Phase 2 (T002-T015)
Covered: 14 of 14 tasks have matching code. Core hierarchy (hierarchy.rs: TaskKind, progress, is_live, 13 Refusal variants matching contracts/api.md, four check_*), Task fields with serde default and a legacy-JSON test (task.rs:491-644), migration 0025, store kind/parent_task/subtasks_of/set_kind/clear_parent_task_links, git base override and BaseMissing, commit_empty/merge_refs, integration.rs (base_for, subtask_facts with 3 queries, progress, live_subtasks, preferred_method; no gh/git), lang.rs refusal_reason/message/body (es+en, all reasons), inbox kinds parent-task-live/ready, api.ts mirror.
Missing: none found.
Extra: none material (parent_task_item inbox rendering is a small addition that supports T014).
Deviates: none.
Untested / not verified:
- core hierarchy tests pass (10). The daemon integration tests were not confirmed to run (the filter matched 0 tests in one target).
- The api.ts check (tsc) could not run: typescript is not installed in app/ in this worktree.
- Minor: subtasks_of loads list_tasks() and filters in Rust (store.rs:499). It works, but it is not a targeted query.
VERDICT: COMPLIANT (no CRITICAL/HIGH findings)
