# WP-05 Code Research: Task Constructor Refactoring

## Files
- `crates/delilah-daemon/src/api.rs:556–690` — `build_task_with` (87 lines; handles NewTask → Task, project/key resolution, hierarchy validation, defaults applied)
- `crates/delilah-daemon/src/api.rs:339–378` — `NewTask` struct (40 fields including key, title, kind, parent_task, project, saving_mode, autopilot, ci_autofix, model, workflow, view, in_checkout, notes)
- `crates/delilah-daemon/src/api.rs:538–547` — `create_task` handler (POST /tasks, calls `build_task_with` then stores)
- `crates/delilah-daemon/src/api.rs:1096–1994` — test module (pattern: async test functions using `app_with_project`, direct `call` to POST /tasks)
- `crates/delilah-daemon/src/api_hierarchy.rs:107–144` — `make_parent_task` (calls raw SQL at L130–131: `UPDATE tasks SET workflow = 'none', saving_mode = 0, autopilot = NULL, ci_autofix = 0, handoff = NULL` — this is what T006's `become_parent_task` will move to store)
- `crates/delilah-daemon/src/handoff.rs:509` — `saving_mode_preference(&AppState) -> bool` (async, reads user preference, needed by `resolve_new_task`)

## Refactoring Shape

**Two new functions to create:**
1. **`new_task(NewTaskSpec { key, title, kind, parent_task, project }, &Defaults) -> Task`** (pure, no async)
   - Takes resolved values only: key already free/valid, parent_task already fetched, project already looked up, defaults already resolved
   - Applies all the field logic from lines 610–689 of current `build_task_with`
   - Returns Task ready to store (no I/O)

2. **`async fn resolve_new_task(app: &AppState, new: NewTask) -> Result<(NewTaskSpec, Defaults), ApiError>`** (async, does lookups)
   - Lines 561–564: title validation, `check_autopilot`
   - Lines 570–608: hierarchy checks, parent lookup (`get_task`), project lookup (`project`), free key generation, key parsing
   - Returns a `NewTaskSpec` with validated/resolved fields and `Defaults` holding project settings and preference
   - Does NOT build the Task yet

**Update existing:**
- `create_task` handler → calls `resolve_new_task(app, body.new)`, then `new_task(spec, defaults)`, then store
- `build_task_with` stays for now but delegates to these (or is inlined into `create_task`)
- Every test in `api_hierarchy.rs` (26 tests covering parent task creation, make-parent-task, attach) must still pass

## Key Defaults to Extract

Looking at L657–687 (what changes between tasks):
- `project: Option<Project>` — from `app.store.project(name)` lookup
- `phased: bool` — `delilah_core::workflow::is_phased(workflow, &p.settings)`
- `autopilot_on: bool` — resolved early
- `saving_mode_preference: bool` — `crate::handoff::saving_mode_preference(app).await`
- `is_parent_task: bool` — from kind after validation

Three key calculations happen post-defaults:
- **workflow**: `ParentTask` → `None`; `Prompt` or message → `Prompt`; else `Project`
- **ci_autofix**: `Some(true)` when explicit, else project's `settings.ci_autofix` unless parent_task/autopilot-off
- **saving_mode**: only phased & !parent_task; falls back to preference if not specified
- **autopilot**: only when explicit true & phased & allowed workflow; forces saving_mode=true
- **in_checkout**: parent_task/subtask always `Some(false)`, normal task keeps request value
- **start_message**: only for Prompt workflow & has message

## Testing Pattern

**Existing (api.rs L1145–1994):**
- `async fn app() -> Router` — in-memory store, no projects
- `async fn app_with_project() -> (Router, AppState)` — adds "web" project (prompt workflow, feature_settings)
- `async fn call(app: &Router, method, uri, body) -> (StatusCode, Value)` — HTTP call, JSON
- Tests create tasks via POST, assert response fields
- Example: `creates_and_lists_tasks` (L1585), `a_backlog_task_takes_saving_mode_from_the_request_or_the_preference` (L1820)

**T011 will add:**
- `new_task` unit tests (no HTTP, no async): build `NewTaskSpec` + `Defaults`, call `new_task`, compare with POST /tasks results
- Test 3 cases: normal Jira task, parent task, subtask
- Compare workflow, saving_mode, ci_autofix, model, in_checkout from `new_task` output against the same fields from POST /tasks with identical inputs

**Existing test invariants to preserve:**
- `a_parent_task_has_no_agent_settings` (L270–279): workflow=none, saving_mode=false, autopilot=null, ci_autofix=false
- `a_subtask_lands_in_the_parent_tasks_project_with_free_keys` (L282–299): subtask.project == parent.project, free keys allocated
- `creating_a_subtask_is_refused_with_the_contract_reason` (L302–370): hierarchy checks (not-a-parent-task, nesting, project-mismatch, parent-task-archived)
- All 26 tests in `api_hierarchy.rs` must pass

## Patterns in Code

**Hierarchy rules baked in** (lines 566–598):
- `delilah_core::hierarchy::check_new_subtask(parent, archived, project)` — validates subtask project/parent kind
- Parent task kind forces `workflow = None`
- Subtask takes parent's project or errors

**Defaults apply asymmetrically:**
- Normal task: all fields from request or computed
- Parent task: forces workflow=None, ignores saving_mode/autopilot/handoff, ci_autofix=false (L666)
- Subtask: takes parent's project, own workflow settings (but subtasks typically have a workflow)

**Async calls in current build_task_with:**
- L584: `app.store.get_task(pk)` for parent lookup
- L601: `app.store.project(name)` for project lookup
- L605: `crate::run::next_free_key(app)` for free key generation
- L683: `crate::handoff::saving_mode_preference(app)` for saving mode default
- Only one call each needed by `resolve_new_task`

## Watch Out

1. **`check_autopilot` (L693–696):** validates `(Some(true), Some(false))` conflict — move to `resolve_new_task` or keep in `new_task`? (Likely resolve_new_task since it's a validation.)

2. **`parse_view` (L565):** string → View enum — stays in `new_task`; happens before defaults.

3. **`resolve_ci_autofix` function:** not visible in this excerpt, find and extract it to compute ci_autofix defaults.

4. **Parent task project field (L598):** if parent exists, overrides new.project; `project_name` modified in-place — `resolve_new_task` must return this resolved name in `NewTaskSpec`.

5. **Tests comparing HTTP response to `new_task` output:** JSON serialization must match — use serde, no custom fields.

6. **`api_hierarchy.rs` L130–131:** This exact UPDATE will move to `store::become_parent_task` in T006; T007 updates the call site. T012 doesn't touch it, but T021 (jira_import) will call the new `new_task` to build tasks, so the shared constructor must work there too.

7. **Free key hand-out (L604–608):** Check that `next_free_key` is idempotent or called once in resolve path, not twice.
