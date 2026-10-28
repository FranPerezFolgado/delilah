# WP-07 Code Researcher: Daemon import handlers and Parent: line

## Files & Patterns

### Route registration (`api.rs`)
- **crates/delilah-daemon/src/api.rs:42–200** — `Router::new()` adds routes; Jira route at :161 `/api/v1/jira/issues/{key}/subtasks` calls `jira::subtasks`
- Pattern: `.route("/path", get(handler).post(handler))` chains methods; new import routes (`GET`/`POST /api/v1/jira/issues/{key}/import`) go next to the subtasks route (~L161)
- Handlers receive `State(app)`, `Path(key)`, optional `Json(body)`; return `Result<Json<T>, ApiError>`

### Existing Jira routes & handlers (`jira.rs`)
- **crates/delilah-daemon/src/jira.rs:169–191** — `subtasks` handler: checks Jira is configured, calls client, maps errors (404 for unknown, 409 if not connected yet)
- **crates/delilah-daemon/src/jira.rs:218–259** — `write_ticket` writes Markdown; currently no `Parent:` line (T023 adds it; note structure: lines for Type, Status, Epic, Components, Link, Description)
- **crates/delilah-daemon/src/jira.rs:261–292** — helper `client()` and `build()` resolve settings + token from AppState secrets; shortcut patterns for missing connection or key validation

### Parent task state management (`api_hierarchy.rs`)
- **crates/delilah-daemon/src/api_hierarchy.rs:109–134** — `make_parent_task` handler: checks open PR and live session, calls `store::become_parent_task()`, emits `Topic::Tasks`
- **crates/delilah-daemon/src/store.rs:~L506–530** (via grep) — existing `set_kind`, existing patterns for transaction builders (`begin_immediate`)
- **crates/delilah-daemon/src/api_hierarchy.rs:42–50** — parent task lookup pattern: `TaskKey::new()`, match on kind, 404 if not ParentTask

### Plan & types (`jira_import.rs`)
- **crates/delilah-daemon/src/jira_import.rs:1–262** — `plan_import` pure function + data classes `TicketInfo`, `ParentAction`, `SubtaskRow`, `SubtaskAction`, `ImportPlan`, `ParentFacts`, `TaskFacts`; tests cover every action path
- `ParentAction::Create | Convert | Existing | Refused {code, message}` (serde `snake_case`)
- `SubtaskAction::Create | Attach | Linked | Conflict {reason}` (serde `snake_case`)
- Decisions: ticket is subtask → `nesting`; parent archived → `archived`; else check `hierarchy::check_make_parent_task`, `check_attach` (R3)

### Integration tests (`tests/parent_tasks.rs`)
- **crates/delilah-daemon/tests/parent_tasks.rs:44–135** — `Env` fixture: real Store + git bare origin + fixture repo; `Config` with `agent_program` (fake claude), `secrets_in_memory: true` for Jira
- **:133–151** — `router()` and `call(method, path, body)` helpers return `(StatusCode, Value)`; `create(kind, parent)` for tasks, `start(key)` for runs
- Pattern: assert `StatusCode::CREATED` on create, parse JSON, call store directly for state checks (`status()`, `workspace()`)
- One parent task test at **:197–270+** (TODO: exact range); reads submodules from parent task workspace (FR-005, R8)

### Jira client (`delilah-integrations/src/jira.rs`)
- **crates/delilah-integrations/src/jira.rs:169–191** — `subtasks(key)` exists; `my_subtask_keys(parent)` to be added (T004) with JQL `parent = KEY AND assignee = currentUser()`; `is_issue_key` check pre-flight (R4)
- Sample data at `..fixtures/jira-search.json`; `SAMPLE_ME = "Fran"`; sample client lives on JiraClient enum

## Checklist for T020–T023

**T020: `preview(app, key)` function**
- [ ] Validate `is_issue_key` (R4), return short-circuit with `linked: false` if invalid
- [ ] Check Jira configured (pattern: `app.store.setting::<JiraSettings>(SETTINGS_KEY)`); short-circuit if not
- [ ] Call Jira: `issue(key)`, `subtasks(key)`, `my_subtask_keys(key)` (T004 must exist)
- [ ] Store queries: `tasks_by_keys()` (T006), `jira_skips_of(parent_key)` (T009)
- [ ] Parent facts: if exists, `get_task()`, check open PR (via `prs_of()`), check session (via `open_sessions()`)
- [ ] If parent exists: call `integration::subtask_facts()` for current children (linked/gone)
- [ ] Call `plan_import()` with all facts; catch Jira errors/timeouts → `linked: false, reason: unreachable|not-found`

**T021: `apply_jira_import(app, plan, request)` function**
- [ ] Build `Task` values with `api::new_task()` (T012), resolving project once outside tx
- [ ] `begin_immediate()` transaction; inside it:
  - [ ] For convert: call `store::become_parent_task(tx, ticket_key)`
  - [ ] For create parent: `insert_task(tx, parent_task)`
  - [ ] Per created subtask: `insert_task(tx, subtask_task)`
  - [ ] Per attach: `set_kind_tx(tx, key, Subtask, parent_key)`
  - [ ] Skip/unskip: `add_jira_skips` / `remove_jira_skips` (T009)
  - [ ] Re-check inside tx: ticket kind, open PR, session (convert); attach targets still normal, never started, same project
- [ ] Return `ImportResult` (data-model); emit `Topic::Tasks` only if created/converted/attached

**T022: Routes in `jira_import.rs` and `api.rs`**
- [ ] `GET /api/v1/jira/issues/{key}/import` → `preview(app, key)` handler
- [ ] `POST /api/v1/jira/issues/{key}/import` → validate request, call `preview()` again, call `apply_jira_import()`, return result
- [ ] Register both in `api.rs:~L161` (next to subtasks route)
- [ ] POST validates: `project` required if `parent.action == Create`; unknown keys in add/skip/unskip → 400 `not-a-subtask`; parent refused → 409

**T023: Parent: line in `jira.rs`**
- [ ] In `write_ticket()` (~L240), after epic block: if issue has `parent`, write `Parent: KEY title` (title from `parent.summary`, key only if title absent)
- [ ] Bilingual strings: `l("Padre", "Parent")`

## Watch out

- **Shared transaction state**: re-checks inside `apply_jira_import` tx (decision 3 from spec review) must use executor pattern like T006
- **Subtasks created in same call**: skip `check_new_subtask` (parent doesn't exist yet); subtasks added to existing parent run it
- **Callers of `set_kind_tx`**: ensure any existing calls still work (T007 changes how it's called)
- **Route order**: new `/jira/issues/{key}/import` and `/jira/issues/{key}/subtasks` must not collide (exact path match)
- **Error codes**: match spec contracts (400 `bad-key`, `project-required`, `not-a-subtask`; 409 with refusal code; 200 for Jira disconnected)
