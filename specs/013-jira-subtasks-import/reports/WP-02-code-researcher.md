# WP-02 Code Researcher: my_subtask_keys

**Tasks**: T003 (tests), T004 (implementation) from specs/013-jira-subtasks-import/tasks.md

---

## Files & why they matter

- `crates/delilah-integrations/src/jira.rs:507–525` — `subtasks()` method: template for `my_subtask_keys`. Validates key with `is_issue_key()`, calls `issue()` to verify parent exists, then filters. Sample path filters by `parent.key == parent` (line 517); Remote uses JQL (line 522).
- `crates/delilah-integrations/src/jira.rs:226–544` — `JiraClient` enum and methods: `issue()` (line 528), `query()` (line 441), `search()` (line 435). Pattern: Sample path uses `query("")` + in-memory filtering; Remote uses JQL strings.
- `crates/delilah-integrations/src/jira.rs:210–224` — `JiraError` enum: includes `BadKey(String)` variant (line 223) returned when `!is_issue_key(parent)`.
- `crates/delilah-integrations/src/jira.rs:786–796` — `is_issue_key(key: &str)` utility: validates `[A-Z]+[A-Z0-9_]*-[0-9]+` format. Used by `subtasks()` at line 507.
- `crates/delilah-integrations/src/jira.rs:143–144` — `Filter::jql()`: shows Remote JQL pattern `assignee = currentUser()`. Use directly in `my_subtask_keys` as `parent = KEY AND assignee = currentUser()`.
- `crates/delilah-integrations/src/jira.rs:168–174` — `Filter::matches()`: sample data filtering by `issue.assignee.as_deref() == Some(SAMPLE_ME)` where `SAMPLE_ME = "Fran"`. Same pattern for `my_subtask_keys`.
- `crates/delilah-integrations/src/jira.rs:900–943` — Test module: `the_sample_has_tickets_with_readable_descriptions()` (L907) and `subtasks_of_a_parent_include_every_assignee_and_status()` (L923) show test structure. Use `JiraClient::sample()`, await async, assert with `matches!()` for errors. BadKey test at line 940.
- `crates/delilah-integrations/fixtures/jira-search.json:1–190` — Sample data: PROJ-1242 (lines 4–31) has two sub-tasks PROJ-1250 (line 165, assigned to "Fran") and PROJ-1251 (line 180, assigned to "Ana"). Both open (`"Por hacer"` and `"En curso"`). Used by `subtasks_of_a_parent` test; extend for T003 fixture if needed.
- `crates/delilah-integrations/src/jira.rs:800–839` — `sample_body()`: extends fixture with 20 dynamically generated subtasks of PROJ-1244. Shows the pattern for adding sample data; use if T003 needs more fixture tickets.

---

## Patterns to follow

1. **Key validation first**: Mimic `subtasks()` line 507: `if !is_issue_key(parent) { return Err(JiraError::BadKey(parent.into())); }`
2. **Verify parent exists**: Call `self.issue(parent).await?` to check parent is real (line 510).
3. **Split by client**: Match on `Self::Sample` vs `Self::Remote` (line 512–525 template).
4. **Sample filtering**: Call `self.query("", MAX_RESULTS, &custom).await?` then filter by both parent and assignee:
   ```rust
   .filter(|i| i.parent.as_ref().is_some_and(|p| p.key == parent) 
               && i.assignee.as_deref() == Some(SAMPLE_ME))
   ```
5. **Remote JQL**: Use `self.query(&format!("parent = {parent} AND assignee = currentUser()"), ...)` with only `key` field. Extract `key` field from response (not a `JiraIssue`; parse directly from JSON).
6. **Return type**: `HashSet<String>` (no duplicates, order irrelevant). Populate from `.key` field of results.

---

## Tests (where & how)

- **Location**: `crates/delilah-integrations/src/jira.rs` in existing `#[cfg(test)] mod tests` block (starts line 900).
- **Style**: Async test with `#[tokio::test]`, use `JiraClient::sample()` fixture, assert with `matches!()` for errors and equality for results.
- **T003 test structure**:
  - Call `jira.my_subtask_keys("PROJ-1242").await.unwrap()` → returns keys of PROJ-1242's subtasks assigned to Fran.
  - Assert result includes only PROJ-1250 (assigned to Fran), not PROJ-1251 (Ana).
  - Test `my_subtask_keys("X; DROP").await` → `matches!(Err(JiraError::BadKey(_)))`.
  - Fixture ticket must have: one open unassigned subtask, one open Fran subtask, one open someone-else's subtask, one done subtask. If PROJ-1242 doesn't, extend sample data or use another parent.

---

## Sample data gaps

- **PROJ-1242 subtasks**: Currently has PROJ-1250 (open, Fran) and PROJ-1251 (open, Ana). **Missing**: an unassigned open subtask and a done subtask. Either:
  - Add two more to the fixture (jira-search.json), or
  - Use PROJ-1246 or PROJ-1244 if they have the right profile, or
  - Use dynamic subtasks from `sample_body()` if testing against PROJ-1244.
- **Check for conflicts**: Ensure the test ticket's parent isn't used elsewhere in Phase 2; T003 fixture is read-only (no Jira writes per FR-013).

---

## Watch out

- `my_subtask_keys` returns keys only, not `JiraIssue` structs. Remote path must parse `key` field from raw JSON, not via `map_issue()`.
- `assignee = currentUser()` is Jira's standard; don't hardcode "Fran" or "currentUser()" in the Remote case—always use the literal string as shown.
- `is_issue_key()` is strict: accepts only `[A-Z]+[A-Z0-9_]*-[0-9]+`. Tests like `"PROJ-1 OR x"` should fail.
- Sample filtering must use `Some(SAMPLE_ME)` equality, not string comparison, because assignee is `Option<String>`.
- T004 extends sample data—must leave `jira-search.json` in a state where T003 passes.

