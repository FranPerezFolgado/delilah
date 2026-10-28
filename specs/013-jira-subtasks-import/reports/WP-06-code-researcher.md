# WP-06 Code Researcher Report: Import Planner (Pure)

**Tasks**: T013–T014 from tasks.md; **Date**: 2026-10-28

## Summary
WP-06 is the pure planner for Jira import: build the decision table `ImportPlan` and the enum types (`ParentAction`, `SubtaskRow`, `SubtaskAction`) that feed UI dialogs and endpoints. Tests are table-driven; implementation is zero I/O, calling only `delilah_core::hierarchy` checks.

---

## Files & Key Types

### Input Types (from coreand daemon)

| File | Type | Usage |
|------|------|-------|
| `crates/delilah-core/src/task.rs:424` | `Task` struct | ticket/subtask facts; fields: `key: TaskKey`, `kind: TaskKind`, `status: TaskStatus`, `project: Option<String>`, `parent_task: Option<TaskKey>`, `in_checkout: Option<bool>` |
| `crates/delilah-core/src/hierarchy.rs:14` | `TaskKind` enum | `Normal`, `ParentTask`, `Subtask`; use `TaskKind::ParentTask` (not bare `parent`) |
| `crates/delilah-integrations/src/jira.rs:786` | `is_issue_key(key: &str) -> bool` | validates Jira key format (e.g., `PROJ-1234`); safe for JQL |
| `crates/delilah-integrations/src/jira.rs:25` | `const SAMPLE_ME: &str = "Fran"` | sample Jira client tests use `"Fran"` as current user |
| `crates/delilah-core/src/hierarchy.rs:81,96,119` | `check_new_subtask`, `check_make_parent_task`, `check_attach` | pure refusal checks; `plan_import` calls these, never copies |

### Output Types (in data-model.md; implement in `crates/delilah-daemon/src/jira_import.rs`)

```rust
#[derive(Serialize, Deserialize)]
pub struct ImportPlan {
  pub ticket: JiraTicket,                    // { key, title, status, is_subtask }
  pub parent: ParentAction,                  // Create | Convert | Existing | Refused { code, message }
  pub subtasks: Vec<SubtaskRow>,
  pub gone: Vec<SubtaskGone>,                // { key, title }
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy)]
#[serde(rename_all = "snake_case")]
pub enum ParentAction {
  Create,
  Convert,
  Existing,
  Refused { code: &'static str, message: &'static str },
}

pub struct SubtaskRow {
  pub key: String,
  pub title: String,
  pub status: String,                        // from Jira
  pub status_category: String,               // "new", "in progress", "done"
  pub assignee: Option<String>,
  pub mine: bool,                            // is assignee == SAMPLE_ME (or current user in real Jira)
  pub task: Option<ExistingTask>,            // { key, kind, status, parent_task, archived }
  pub action: SubtaskAction,
  pub skipped: bool,
  pub selected: bool,                        // default tick
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy)]
#[serde(rename_all = "snake_case")]
pub enum SubtaskAction {
  Create,
  Attach,
  Linked,
  Conflict { reason: &'static str },         // "started", "other-project", "other-parent", "parent-task"
}
```

### Test Patterns

| Location | Pattern | Example |
|----------|---------|---------|
| `crates/delilah-integrations/src/jira.rs:900-943` | Table tests with sample client | `let jira = JiraClient::sample(); let kids = jira.subtasks("PROJ-1242").await?;` |
| `crates/delilah-core/src/hierarchy.rs:159-200` | Pure logic tests | `fn task(key: &str, kind: TaskKind, project: Option<&str>) -> Task { let mut t = Task::new(TaskKey::new(key), "t", "feature"); t.kind = kind; … }` |
| Daemon patterns | Module `#[cfg(test)] mod tests { … }` with fixtures | See `crates/delilah-daemon/src/jira.rs:294-350` |

---

## Decision Points for T013–T014

**T013**: Write *table tests* covering every parent decision (`create`, `convert`, `existing`, `refused` with each refusal code from spec 012 + `nesting` for Jira sub-task tickets) and every sub-task action (`create`, `attach`, `linked`, `conflict` with reasons). Test `selected` defaults and `gone` rows.

**T014**: Implement `ImportPlan`, `ParentAction`, `SubtaskRow`, `SubtaskAction` enums/structs with `serde` `#[serde(rename_all = "snake_case")]` (contracts/api.md); pure `plan_import(ticket, task_with_key, ticket_has_open_pr, ticket_has_session, jira_subtasks, delilah_tasks_by_key, skips, my_keys) -> ImportPlan` calling only `hierarchy::check_make_parent_task`, `check_new_subtask`, `check_attach`. No I/O, no store access.

---

## Related Symbols

- `delilah_core::hierarchy::Refusal` — enum of all refusal codes
- `delilah_integrations::JiraIssue`, `JiraClient::sample()` — sample Jira data
- `crates/delilah-daemon/src/store.rs` (T005–T006) — `become_parent_task`, `tasks_by_keys`, inputs to preview
- Tasks T020 (preview) and T021 (apply) use the plan; T022–T027 are endpoints and UI

---

## Test Data Ready Now

- Sample Jira client (SAMPLE_ME = "Fran") has `PROJ-1242` with sub-tasks `PROJ-1250` (Fran), `PROJ-1251` (Ana).
- Extend sample data with one ticket that is itself a sub-task (for `nesting` refusal test).
- `TaskKey::parse()` validates keys; `is_issue_key()` for JQL safety.

---

## Checklist

- [ ] T013 tests: all parent cases, all sub-task actions, `selected` logic, `gone` rows, Jira sub-task ticket → `refused(nesting)`
- [ ] T014 types: `ImportPlan`, `ParentAction` (`Create | Convert | Existing | Refused`), `SubtaskRow`, `SubtaskAction` with `serde` `snake_case`
- [ ] T014 function: `plan_import(…) -> ImportPlan`, pure, calls only `check_make_parent_task`, `check_new_subtask`, `check_attach`
