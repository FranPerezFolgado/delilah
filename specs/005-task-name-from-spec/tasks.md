# Tasks: Task name and branch from the first analysis

**Input**: Design documents from `specs/005-task-name-from-spec/`
**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/api.md, quickstart.md

**Tests**: test-first. Write each test task before its implementation and see it fail.

**Checks**: `cargo test -p <crate>`; `cd app && npx tsc --noEmit && npx vitest related --run <files>`; at the end `cargo test --workspace`, `cargo test --manifest-path app/src-tauri/Cargo.toml` and `npx vitest run`.

## Format: `[ID] [P?] [Story] Description`

## Phase 1: Setup

None: no new dependencies.

---

## Phase 2: Foundational (blocks every story)

**Purpose**: types, storage, the spec-folder scan and the triggers both stories hang from.

- [X] T001 [P] Write failing tests in `crates/delilah-core/src/task.rs`: `TitleSource` (`initial`/`spec`/`manual`, serde lowercase, default `manual`); `Task` deserialises without `title_source`/`previous_title`/`branch_error` (defaults `manual`/`None`/`None`); `Task::new` gives `manual`.
- [X] T002 Implement T001 in `crates/delilah-core/src/task.rs`: `Task.title_source: TitleSource` (`#[serde(default)]`), `Task.previous_title: Option<String>`, `Task.branch_error: Option<String>`.
- [X] T003 [P] Add `crates/delilah-daemon/migrations/0015_task_branch_from_spec.sql` exactly as in data-model.md: `tasks.title_source TEXT NOT NULL DEFAULT 'manual'`, `tasks.previous_title TEXT`, `tasks.branch_error TEXT`, `workspaces.spec_dirs_at_start TEXT`.
- [X] T004 Write failing store tests in `crates/delilah-daemon/src/store.rs` (tests module): a created task reads back the three new fields; a task row from before the migration reads `manual`; `set_title_source(key, source, previous_title)`, `set_branch_error(key, Option<&str>)` persist; `set_title(key, title)` works on a running task, refuses an archived one, sets `title_source = manual` and clears `previous_title`.
- [X] T005 Implement T004 in `crates/delilah-daemon/src/store.rs`: select/insert the new columns in every task query; `create_task` writes `title_source`; `set_title_source`, `set_branch_error`, `set_title`.
- [X] T006 [P] Write failing tests in `crates/delilah-daemon/src/store_runs.rs`: `save_workspace` with `branch = ""` and `spec_dirs_at_start = Some([...])` round-trips; `set_workspace_branch(key, branch)` updates only the branch; a workspace saved without `spec_dirs_at_start` reads `None`.
- [X] T007 Implement T006 in `crates/delilah-daemon/src/store_runs.rs` (`spec_dirs_at_start` as JSON text; `Workspace` gets it through a store-side struct or a separate getter `spec_dirs_at_start(key)`; keep `delilah_git::Workspace` unchanged apart from `has_branch()`).
- [X] T008 [P] Write failing tests in `crates/delilah-git/src/workspace.rs`: `Workspace::has_branch()` (`""` → false); new `spec_dirs(root) -> Vec<String>` lists `specs/*/` directories holding a `spec.md`; `current_branch(dir)` returns `None` on a detached HEAD.
- [X] T009 Implement T008 in `crates/delilah-git/src/workspace.rs`.
- [X] T010 Create `crates/delilah-daemon/src/branching.rs` (register in `lib.rs`): `new_spec_folder(app, key) -> Option<(label, dir)>` = first folder from `spec_dirs` over the main repo and the chosen submodules that is not in `spec_dirs_at_start` (labels `.` / submodule path); with failing tests first in the same file's tests module (old folders ignored, a submodule's new folder found, a folder without `spec.md` ignored).
- [X] T011 Add the trigger skeleton in `crates/delilah-daemon/src/branching.rs`: `on_activity(app, key)` and `recover(app, task)`, each a no-op unless the task is free (`LIBRE-` key) and either branchless or `title_source == initial`; a per-task `tokio::sync::Mutex` map in `AppState` (`crates/delilah-daemon/src/state.rs`) so two triggers never run together. Call `on_activity` from `phases::on_hook` for every `Signal::Working` and on `TurnEnded` (`crates/delilah-daemon/src/phases.rs`), and `recover` from `scheduler::tick` and once at daemon start (`crates/delilah-daemon/src/scheduler.rs`, `main.rs`).
- [X] T012 Record `spec_dirs_at_start` for every free task in `start_run` (`crates/delilah-daemon/src/run.rs`): scan the main repo (and chosen submodules) after the workspace exists, before the first session launches; create free tasks with `title_source = initial` (new tasks and backlog tasks created from now on, `create_task` paths in `run.rs` and the backlog create in `api.rs`/`proposals.rs`).

**Checkpoint**: `cargo test -p delilah-core -p delilah-git -p delilah-daemon` green; no behaviour change yet.

---

## Phase 3: User Story 1 — The task takes its name from the analysis (P1) 🎯 MVP

**Goal**: after the first spec is written, a free task's title becomes the spec's, once; the old title can be restored; running tasks' titles can be edited.

**Independent test**: preview, free task "cosas de ramas", run specify: the title changes on the board, Sidebar and task screen; "Restaurar" brings it back; a second spec doesn't rename it (quickstart 1 and 7).

- [X] T013 [P] [US1] Write failing tests in `crates/delilah-daemon/src/branching.rs`: `spec_title(path)` reads `# Feature Specification: <name>` (trimmed); `None` for a missing heading, empty name, `[FEATURE NAME]` or any `[…]`-only name.
- [X] T014 [US1] Implement `spec_title` in `crates/delilah-daemon/src/branching.rs`.
- [X] T015 [US1] Write failing tests in `crates/delilah-daemon/src/branching.rs`: `retitle(app, key)` on an `initial` free task with a new spec folder sets the title, `title_source = spec`, `previous_title = old`, emits `Topic::Tasks`; does nothing for `spec`/`manual` tasks, Jira tasks, a placeholder heading, or an old folder; `on_activity` and `recover` call it.
- [X] T016 [US1] Implement `retitle` and wire it into `on_activity`/`recover` in `crates/delilah-daemon/src/branching.rs`.
- [X] T017 [US1] Write failing API tests in `crates/delilah-daemon/src/api.rs` (tests module): `PATCH /api/v1/tasks/{key}` with only `title` on a running task → 200, `title_source = manual`, `previous_title = null`; with `project`/`workflow`/`notes`/`in_checkout` on a running task → 409 as today; on an archived task → 409; empty title → 400.
- [X] T018 [US1] Implement T017 in `crates/delilah-daemon/src/api.rs` `update_task`: title-only changes go through `store.set_title`, the rest through `update_backlog_task` (which also marks the title `manual` when it changes it).
- [X] T019 [P] [US1] Add `title_source`, `previous_title`, `branch_error` to `Task` in `app/src/api.ts` (contracts/api.md).
- [X] T020 [US1] Write failing tests in `app/src/components/TaskScreen.test.tsx`: the header title is editable in place (Enter saves via `updateTask({title})`, Escape cancels, empty is refused); with `previous_title` set it shows "Antes: <previous>" and "Restaurar", which patches the previous title; without it, neither shows.
- [X] T021 [US1] Implement T020 in `app/src/components/TaskScreen.tsx` and the English strings in `app/src/i18n/en.ts`.

**Checkpoint**: US1 works end to end in a preview with today's branches untouched.

---

## Phase 4: User Story 2 — The branch is named after the analysis, not the key (P1)

**Goal**: free tasks start without a branch; Spec Kit's hook (or Delilah from the spec folder, or the title as a last resort) creates it; parents get it in every chosen submodule; nothing lands on the base branch; failures show in the Bandeja with a retry.

**Independent test**: quickstart 1–6 on the fixture repo and parent, checkout and worktree mode.

### Git layer

- [X] T022 [P] [US2] Write failing tests in `crates/delilah-git/src/workspace.rs`: `create_workspace` with `branch: None` for a repo makes a detached worktree and no branch; for a parent leaves the chosen submodules detached; `use_checkout` with `branch: None` refuses a dirty folder, switches nothing and returns `previous` including `"."` for a parent; `leave_checkout` puts the parent back too; `apply_branch(dir, name)` = `checkout -b` keeping uncommitted files, error if it exists; `branch_exists(dir, name)` sees local and remote-tracking branches; with `branch: Some(..)` everything behaves as before.
- [X] T023 [US2] Implement T022 in `crates/delilah-git/src/workspace.rs`: `WorkspaceSpec.branch: Option<String>`, `apply_branch`, `branch_exists`; update callers of `WorkspaceSpec` (`run.rs`, tests).

### Daemon

- [ ] T024 [US2] Write failing tests in `crates/delilah-daemon/src/branching.rs` for `slug(title)`: accents folded, lower-case, non-alphanumerics → `-`, ≤ 4 words and 40 chars, no leading/trailing `-`, empty → `task`.
- [ ] T025 [US2] Implement `slug` in `crates/delilah-daemon/src/branching.rs`.
- [ ] T026 [US2] Write failing tests in `crates/delilah-daemon/src/branching.rs` for `ensure(app, key, why)` (`why`: `Activity` | `Guard` | `Retry`) on temp repos: (a) worktree switched by the "hook" (test does `checkout -b 006-x`) → adopted as `006-x`, workspace record updated; (b) checkout mode switched away from `previous` → adopted; (c) new spec folder `005-y`, no branch → `005-y` created, uncommitted spec files kept; (d) `005-y` taken locally or as `origin/005-y` → `005-y-2`; (e) `Guard` without spec folder → `slug(title)`; `Activity` without spec folder → nothing; (f) parent: branch in the parent root, then each chosen submodule switched; a submodule that fails → `branch_error` names it, the parent's branch is recorded, a retry finishes the submodule; (g) a task with a branch, a Jira task, or a task started before the feature → no-op; (h) running it twice → same result; (i) success clears `branch_error` and emits `Topic::Tasks`.
- [ ] T027 [US2] Implement `ensure` in `crates/delilah-daemon/src/branching.rs` (research R2) and call it from `on_activity` (`Activity`) and `recover` (`Activity`).
- [ ] T028 [US2] Make free tasks start branchless in `start_run` (`crates/delilah-daemon/src/run.rs`): `WorkspaceSpec.branch = None` for `LIBRE-` keys without an explicit `req.branch`; Jira keys keep `feature/<KEY>`; add the R8 sentence to the first prompt of a branchless task (both languages, via `app.lang().pick`); test in `run.rs` tests that a free task's workspace has `branch == ""` and a Jira one `feature/<KEY>`.
- [ ] T029 [P] [US2] Write failing tests in `crates/delilah-agents/src/claude.rs`: `settings(..)` with a guard URL adds a second `PreToolUse` entry, matcher `Edit|Write|MultiEdit|NotebookEdit|Bash`, `curl -m 15` to `/hooks/claude/guard?task=…&session=…`, alongside the AskUserQuestion one; without it, settings are as before.
- [ ] T030 [US2] Implement T029 in `crates/delilah-agents/src/claude.rs` (a new `guard: bool` parameter or options struct) and pass it from `launch_session` in `crates/delilah-daemon/src/run.rs` when the task's workspace has no branch.
- [ ] T031 [US2] Write failing tests for the guard handler in `crates/delilah-daemon/src/branching.rs`: an edit under `specs/` or `.specify/` → allow without `ensure`; an edit elsewhere or Bash with `git commit`/`git push`/`gh pr create`/`git merge`/`git rebase` → `ensure(Guard)` runs first, then allow; `ensure` failing → deny JSON with the reason; a task that already has its branch → allow at once; other Bash commands → allow.
- [ ] T032 [US2] Implement the guard handler in `crates/delilah-daemon/src/branching.rs` and route `POST /hooks/claude/guard` in `crates/delilah-daemon/src/hooks.rs`/`api.rs` (same owner-name check as `claude_hook`).
- [ ] T033 [US2] Write failing API tests in `crates/delilah-daemon/src/api.rs`: `POST /api/v1/tasks/{key}/branch` → 200 with the task (`ensure(Retry)`); 404 unknown; 409 for a Jira task or one already fully on its branch.
- [ ] T034 [US2] Implement T033 in `crates/delilah-daemon/src/branching.rs` + route in `crates/delilah-daemon/src/api.rs`.

### Compatibility

- [ ] T035 [P] [US2] Write failing tests and implement in `crates/delilah-daemon/src/specs.rs`: with `branch == ""`, `find_spec_dir` skips the branch-name match and `changed_paths` (no git error, returns the key-named folder if any).
- [ ] T036 [P] [US2] Write failing tests and implement in `crates/delilah-daemon/src/pr_api/open.rs` and `crates/delilah-daemon/src/pr_loop/mod.rs`: opening a PR for a branchless task → 422 "la tarea aún no tiene rama" / "the task has no branch yet"; the PR loop skips branchless tasks.
- [ ] T037 [P] [US2] Write failing tests and implement in `crates/delilah-daemon/src/archive.rs` (and `delilah-git` `archive_workspace` if needed): archiving and deleting a branchless worktree task (clean → removed; dirty → refused unless force) and a branchless checkout task (goes back to `previous`, including the parent).

### Bandeja

- [X] T038 [US2] Write failing tests in `crates/delilah-daemon/src/inbox.rs`: a task with `branch_error` yields an item `{ kind: "branch", task, title, text }`; none once cleared; none for archived tasks.
- [X] T039 [US2] Implement T038 in `crates/delilah-daemon/src/inbox.rs` (`ItemKind::Branch`).
- [X] T040 [P] [US2] Add `"branch"` to the inbox item kind and `retryBranch(key)` to `app/src/api.ts`.
- [X] T041 [US2] Write failing tests in `app/src/components/InboxView.test.tsx`: a `branch` item shows "No se pudo crear la rama: <text>" and "Reintentar", which calls `retryBranch`; then implement in `app/src/components/InboxView.tsx` and `app/src/i18n/en.ts`.

**Checkpoint**: quickstart 1–6 pass in a preview; `cargo test --workspace` green.

---

## Phase 5: Polish & cross-cutting

- [ ] T042 [P] Update `docs/architecture.md` (Workspaces: branchless start, `branching.rs`, guard hook; data: new columns), `docs/status.md` (feature landed), `docs/decisions.md` (new entry: the branch comes from the analysis; worktree folder keeps the key; guard rationale; title once with restore) and `README.md` (user-facing behaviour).
- [ ] T043 [P] Tick the answered items of `specs/005-task-name-from-spec/checklists/workspace.md` with a pointer to research.md.
- [ ] T044 Run the full checks: `cargo test --workspace`, `cargo test --manifest-path app/src-tauri/Cargo.toml`, `cd app && npx tsc --noEmit && npx vitest run`; `cargo clippy --workspace` clean.
- [ ] T045 Walk quickstart.md in a preview (`scripts/preview.sh --fresh`), never on 7420.
- [ ] T046 Version bump **minor** from `main`'s version in the four places in `docs/development.md#releasing` plus the lockfiles, as its own commit `chore: bump version to X.Y.0`.

---

## Dependencies & execution order

- Phase 2 blocks both stories. Inside it: T001→T002; T003 before T004–T007; T004→T005; T006→T007; T008→T009; T010–T012 after T005, T007, T009.
- **US1** (T013–T021) only needs Phase 2. **US2** (T022–T041) needs Phase 2; T013–T016 are not needed by US2, so the stories can run in parallel, but both edit `branching.rs`: run their `branching.rs` tasks one after another (US1 first).
- Inside US2: T022→T023 before T026–T028 and T037; T024→T025 before T026; T026→T027; T029→T030; T031→T032; T033→T034; T038→T039; T040 before T041.
- Polish after both stories; T046 last.

## Parallel examples

- Phase 2: T001, T003, T006, T008 together (core, migration, store_runs, git).
- US1: T019 (api.ts) alongside T013–T018.
- US2: T022 (git), T029 (agents), T035, T036, T037 (different files) together once T023 is in; T040 alongside the daemon tasks.

## Implementation strategy

1. Phase 2 → US1: the board shows meaningful names; branches unchanged. Shippable on its own.
2. US2: branchless start, `ensure`, guard, compatibility, Bandeja.
3. Polish, docs, version bump, PR.
