## WP-06 — done
Tasks:    T013 ✓, T014 ✓
Files:    crates/delilah-daemon/src/jira_import.rs — pure `plan_import` + types + tests
Tests:    parent_actions, parent_refusals, subtask_actions, selected_defaults, gone_lists_children_missing_from_jira, attach_when_parent_is_created_or_converted, serializes_like_the_contract; `cargo test -p delilah-daemon jira_import` passes (8 incl. a WP-03 store test)
Notes:
- Signature: plan_import(ticket:&JiraIssue, parent:Option<&ParentFacts>, jira_subtasks:&[JiraIssue], tasks:&HashMap<String,TaskFacts>, children:&[Task], skips:&HashSet<String>, my_keys:&HashSet<String>) -> ImportPlan. ParentFacts{task,archived,has_open_pr,has_session}; TaskFacts{task,archived,started}.
- Sub-task attach is checked against a hypothetical parent task (the ticket as parent), so it works for create/convert.
- Serde: ParentAction tagged "action"; SubtaskAction flattened into SubtaskRow (so non-conflict rows omit `reason` rather than null; WP-07 can add it if the UI needs null). Refused.message is String; ParentAction/SubtaskAction derive Serialize only (no Deserialize, static str reasons).
- Refusal codes: nesting, open-pr, running-session, archived, in-checkout. Archived parent task (any kind) -> refused(archived).
- Module has `#![allow(dead_code)]` until WP-07 wires it.
- The first full run was blocked briefly by other packages' in-progress api.rs tests; resolved on retry.
