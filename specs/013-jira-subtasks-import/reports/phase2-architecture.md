# Phase 2 architecture review (2de38ee..HEAD, crates/delilah-daemon)

Verdict: no CRITICAL or HIGH. Structure follows plan.md; layering is sound (pure planner, executor-taking store primitives, rules only from delilah_core::hierarchy).

[MEDIUM] crates/delilah-daemon/src/jira_import.rs:~150
Problem: For a ticket with no Delilah task (Create), `hypothetical` is built with `project = None`; `check_attach` ends in `same_project(task.project, parent.project)`, so attach candidates with a project may be reported as conflict ("other-project") although the dialog's chosen project would match. The planner has no project input.
Fix: Pass the target project (or Option<&str>) into `plan_import` and set `hypothetical.project` from it (or the parent task's); add a table test for Create + existing normal task in that project. The same planner is used by the GET preview, which has no project yet, so decide the semantics (treat unknown as "no mismatch") explicitly.

[MEDIUM] crates/delilah-daemon/src/store.rs:5 and store/jira_skips.rs
Problem: `add_jira_skips`/`remove_jira_skips` are free functions re-exported from store.rs while the rest of jira_skips.rs is `impl Store`; `set_kind_tx`/`become_parent_task` are free functions too. Mixed styles for store access, and `store::` now exports a few loose functions that bypass the `Store` facade (apply code will reach into `store.pool` as the actions.rs test does).
Fix: Keep executor-taking helpers but group them (e.g. `pub(crate) mod tx { ... }` or a documented "transaction primitives" section) and keep `Store::begin_immediate` as the only way to open the tx; do not let jira_import touch `store.pool` directly.

[LOW] crates/delilah-daemon/src/store.rs:528
Problem: `begin_immediate` duplicates the `begin_with("BEGIN IMMEDIATE")` used 5 times elsewhere (store.rs:99,285,597, store_prs.rs:205, store_sessions.rs:149,191).
Fix: Make those callers use it.

[LOW] crates/delilah-daemon/src/api.rs (NewTaskSpec / Defaults / new_task)
Problem: Split is clean, but `Defaults` is a 9-field pub(crate) struct with a `Default` impl only used by tests/import; `autopilot` and `saving_mode` interact inside `new_task` (autopilot forces saving_mode) while parent-task suppression lives in `resolve_new_task`, so an importer building `Defaults` by hand can bypass the parent-task invariants (ci_autofix/saving_mode/workflow 'none').
Fix: Add `Defaults::for_kind(kind, ...)` or assert in `new_task` that `kind == ParentTask` forces workflow "none", saving_mode/ci_autofix/autopilot off, so the invariant lives in one place (new_task_matches_post_tasks covers it only for hand-picked inputs).

[LOW] crates/delilah-daemon/src/api_hierarchy.rs:130
Problem: `make_parent_task` now does the settings reset and kind change in one statement but drops the old `set_kind` return and re-reads via `known()`; behaviour is equal, but `become_parent_task` unconditionally sets `parent_task = NULL` while the old path passed None too: consistent. No issue beyond the extra read.
Fix: None required.

[LOW] crates/delilah-daemon/src/jira_import.rs:7
Problem: Module-wide `#![allow(dead_code)]` until the routes land; refusal codes/messages (`refusal_text`) duplicate text that api_hierarchy produces for the same Refusals.
Fix: Remove the allow in the wiring phase; reuse the existing Refusal-to-ApiError mapping instead of a second copy of the strings.

Positive: actions.rs delete path pairs `delete_jira_skips` with `clear_parent_task_links` (single call site), archive keeps skips (tested). `tasks_by_keys` binds params safely (only `?` marks interpolated). Migration table not touched in this range (0026 presumably earlier).
