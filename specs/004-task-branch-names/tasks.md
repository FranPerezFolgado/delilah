# Tasks: Task branch names that describe the task

**Input**: [plan.md](plan.md), [spec.md](spec.md), [research.md](research.md), [data-model.md](data-model.md), [contracts/api.md](contracts/api.md), [quickstart.md](quickstart.md)

**Tests**: test-first. Each phase's tests are written and seen failing before its code.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: can run in parallel (different files, no pending dependency)
- **[Story]**: US1 (branch named after the task), US2 (see/edit in the dialog), US3 (branch on the task view)

---

## Phase 1: Setup

- [X] T001 Add `claude_bin: PathBuf` to `Config` in `crates/delilah-daemon/src/config.rs`: read from env `DELILAH_CLAUDE_BIN`, default `claude`; settable in tests like the other fields (follow how `DELILAH_GH_BIN` is handled in `crates/delilah-daemon/src/gh/mod.rs`)

---

## Phase 2: Foundational (pure rules, git, Claude adapter)

### Tests first

- [X] T002 [P] Unit tests in `crates/delilah-core/src/branch.rs` (`#[cfg(test)]`). `slug`: accents (`"Añadir réplica ÜÇß"`→`anadir-replica-ucss`), symbols/emojis dropped, runs of `-` collapsed, no leading/trailing `-`, cut at a word boundary within 50 characters. `title_slug`: filler words dropped (`"Las tareas deben crearse con nombre de rama"`→`tareas-deben-crearse-nombre-rama`), at most 6 words, filler words kept when nothing else is left, empty for `"🚀 !!"`. `compose`: `compose(Some("PROJ-12"), "retry-failed-payments", false)`→`feature/PROJ-12-retry-failed-payments`, free → `feature/retry-failed-payments`. `custom`: `"Arreglo del login"`→`feature/arreglo-del-login`, `"feature/feature/x"`→`feature/x`, `"fix/login"`→`feature/fix-login`, `"///"`→`None`. `fallback`: `("PROJ-1", false)`→`feature/PROJ-1`, `("LIBRE-6", true)`→`feature/libre-6`
- [X] T003 [P] Unit tests in `crates/delilah-agents/src/branch_name.rs` (`#[cfg(test)]`, mirroring `suggest.rs` tests). The prompt contains the title, the notes when given, and asks for `{"name": ...}` in 2–4 English words. `args` contain `-p`, `--model haiku`, `--output-format json`, `--no-session-persistence`, `--strict-mcp-config`, `--setting-sources user` and `--tools ""` (empty, i.e. no tools). `parse` returns the name from JSON inside prose, an `Err` for `is_error: true` (with the message), an `Err` for prose with no JSON, and an `Err` for an empty `name`
- [X] T004 [P] Integration tests in `crates/delilah-git/tests/workspace.rs` (fixture repos as the existing tests build them, plus a local bare remote). `branch_taken`: false for a new name, true for a local branch, true for a branch only on the remote (`refs/remotes/origin/<name>` after fetch). `free_branch`: returns the base when free; `-2` when the base is taken; `-3` when the base and `-2` are taken; a name taken in only one of several dirs (two submodule repos) still gets a suffix

### Implementation

- [X] T005 [P] Implement `crates/delilah-core/src/branch.rs` (`slug(text, max)`, `title_slug(title)`, `compose(key: Option<&str>, short, free)`, `custom(input) -> Option<String>`, `fallback(key, free)`, `PREFIX = "feature/"`, `MAX_SHORT = 50`) with a small transliteration table (research R3), and export it in `crates/delilah-core/src/lib.rs`
- [X] T006 [P] Implement `crates/delilah-agents/src/branch_name.rs` (`MODEL = "haiku"`, `prompt(title, notes)`, `args(prompt)`, `parse(output) -> Result<String, String>`; reuse `suggest::env()` for the env), and export it in `crates/delilah-agents/src/lib.rs`
- [X] T007 [P] Implement `branch_taken(dir, name, opts) -> Result<bool, GitError>` (`refs/heads/<name>` or any `refs/remotes/*/<name>` via `for-each-ref`) and `free_branch(dirs: &[PathBuf], base, opts) -> Result<String, GitError>` (`-2`…`-99`, then base + `-<unix time>`) in `crates/delilah-git/src/workspace.rs`, and export them from `crates/delilah-git/src/lib.rs` if it re-exports

**Checkpoint**: `cargo test -p delilah-core -p delilah-agents -p delilah-git` green.

---

## Phase 3: User Story 1 — a started task gets a branch named after it (P1) 🎯 MVP

**Goal**: `start_run` without `branch` names the branch from Claude, the title or the key; any name is unique; stored as today.

**Independent test**: `cargo test -p delilah-daemon` start tests with a fake claude script; quickstart steps 1, 2, 4, 5, 7.

### Tests first

- [X] T008 [US1] Daemon tests in `crates/delilah-daemon/src/branch_name.rs` (`#[cfg(test)]`, writing a fake `claude` shell script into a temp dir and pointing `Config::claude_bin` at it). `propose` with a fake that prints `{"is_error":false,"result":"{\"name\":\"retry failed payments\"}"}` gives `feature/PROJ-12-retry-failed-payments`, source `claude`. A fake that exits 1, prints `is_error: true`, or sleeps past the limit gives the title slug, source `title`. An unusable title and a failing Claude give `fallback`, source `key`, or `None` for a free task with no key. A taken name in the fixture repo gets `-2`
- [X] T009 [US1] Tests in `crates/delilah-daemon/src/run.rs` (`#[cfg(test)]`, existing start-test helpers). A free task started without `branch` and a fake Claude naming it `task branch names` works on `feature/task-branch-names` (`git branch --show-current` in the workspace, and `store.workspace(key).branch`). A Jira `PROJ-12` task works on `feature/PROJ-12-…`. With Claude failing, a free `LIBRE-n` task with title `"!!"` works on `feature/libre-n`. In checkout mode with `feature/x` already existing, the task gets `feature/x-2` and the existing branch is untouched

### Implementation

- [X] T010 [US1] Implement `crates/delilah-daemon/src/branch_name.rs`. `pub struct Proposal { branch: Option<String>, source: Source }` with `Source` serialised as `claude|title|key`. `pub async fn propose(app, project: &Project, key: Option<&str>, title, notes, free, submodules, limit: Duration) -> Result<Proposal, ApiError>` runs `Command::new(&app.config.claude_bin)` with `env_clear().envs(suggest::env())`, `branch_name::args`, `current_dir(std::env::temp_dir())`, stdin null, `kill_on_drop`, `tokio::time::timeout(limit)`, then falls back through `title_slug` and `fallback`, and finally `free_branch` (via `spawn_blocking`) over the repos `dirs_of(project, submodules)`: the project path for `repo`, `path/<sub>` for each submodule of a `parent`. Log the source at info. Register `mod branch_name` in `crates/delilah-daemon/src/main.rs`/`lib.rs`
- [X] T011 [US1] In `crates/delilah-daemon/src/run.rs` `start_run`, before `app.transition(&key, Event::Start)`: `req.branch` → `delilah_core::branch::custom` (400 `"the branch name has nothing usable"` if `None`), else `propose(..., START_NAME_LIMIT = 5 s)` with the task's key (`free` from `req.free`); then `free_branch` over the same dirs; use the result in `WorkspaceSpec.branch`. Update the `RunRequest.branch` doc comment ("Defaults to a name Claude proposes from the title, as `feature/…`; always normalised and made unique")
- [X] T012 [P] [US1] Update the `--branch` help text in `crates/delilah-cli/src/main.rs` ("Defaults to a name Claude proposes from the title (feature/…); a typed one is normalised and made unique")

**Checkpoint**: `cargo test -p delilah-daemon` green; quickstart 1 (via API), 2, 4, 5, 7.

---

## Phase 4: User Story 2 — see and change the name before starting (P2)

**Goal**: the endpoint, and a "Rama" field in the start dialog that is proposed, editable and sent as `branch`.

**Independent test**: vitest on the dialog; quickstart steps 1 and 3 in the UI.

### Tests first

- [X] T013 [US2] HTTP tests for `POST /api/v1/projects/{name}/branch-name` in `crates/delilah-daemon/src/branch_name.rs` (`#[cfg(test)]`, `crate::api::router(app).oneshot(...)` as in `asks.rs`/`session_api.rs` tests): 200 `{branch, source}` with a fake Claude; 200 with source `title` when Claude fails; 400 for an empty title; 404 for an unknown project. Plus a `start_run` test: `branch: "Arreglo del login"` → `feature/arreglo-del-login`, `branch: "///"` → 400 and a backlog task stays in the backlog
- [X] T014 [P] [US2] Vitest in `app/src/components/NewTaskDialog.test.tsx` (mock `api` as other component tests do, fake timers):
  - typing a title calls `api.proposeBranch` once after 1 s and fills the "Rama" field with the proposal and its source label
  - after the developer edits the field, a later proposal doesn't overwrite it; clearing the field lets proposals fill it again
  - submit sends `branch` when the field is non-empty and omits it when empty
  - a stale proposal (older request) is ignored
  - the `existing` and `fromSession` variants also show the field

### Implementation

- [X] T015 [US2] Add the handler `propose_branch` (request `{key?, title, notes?, free, submodules[]}`, 10 s limit, 400 for an empty title) to `crates/delilah-daemon/src/branch_name.rs`, and the route `POST /api/v1/projects/{name}/branch-name` in `crates/delilah-daemon/src/api.rs`
- [X] T016 [US2] In `app/src/api.ts`: `RunRequest.branch?: string`, `export interface BranchProposal { branch: string | null; source: "claude" | "title" | "key" }`, and `api.proposeBranch(project, { key?, title, notes?, free, submodules })` → `post<BranchProposal>(`/api/v1/projects/${project}/branch-name`, …)`
- [X] T017 [US2] In `app/src/components/NewTaskDialog.tsx`, add the "Rama" field: a fixed `feature/` prefix and an input with the part after it. Request the proposal debounced 1 s on (project, key, title, origin, picked submodules), keyed so stale answers are dropped. Show "Proponiendo…" while waiting and "propuesta por Claude" / "a partir del título" / "a partir de la clave" after. An `edited` flag stops proposals overwriting the developer's name, and clearing the field resets it. Send `branch: "feature/" + value` on submit when non-empty. Use `t()` for all strings
- [X] T018 [P] [US2] Add the dialog's new strings to `app/src/i18n/en.ts` ("Rama" → "Branch", "Proponiendo…" → "Proposing…", "propuesta por Claude" → "proposed by Claude", "a partir del título" → "from the title", "a partir de la clave" → "from the key", "automática si la dejas vacía" → "automatic if left empty")

**Checkpoint**: `cargo test -p delilah-daemon`, `cd app && npx tsc --noEmit && npx vitest related --run src/components/NewTaskDialog.tsx` green.

---

## Phase 5: User Story 3 — the branch is visible on the task (P3)

**Goal**: `Task.branch` from the stored workspace, shown in the task view.

**Independent test**: store test + vitest on `TaskView`; quickstart step 1 (task view).

### Tests first

- [X] T019 [US3] Store tests in `crates/delilah-daemon/src/store.rs` (`#[cfg(test)]`): `get_task` and `list_tasks` return `branch: Some("feature/PROJ-1")` after `save_workspace`, and `None` for a backlog task and after `delete_workspace`. Serialising a task without a branch omits the field
- [X] T020 [P] [US3] Vitest: `TaskView` shows the branch (monospace) when `task.branch` is set and nothing when it's absent, in `app/src/components/TaskScreen.test.tsx` (or a new `app/src/components/TaskView.test.tsx` if `TaskScreen` doesn't render `TaskView`)

### Implementation

- [X] T021 [US3] Add `branch: Option<String>` (`#[serde(default, skip_serializing_if = "Option::is_none")]`) to `Task` in `crates/delilah-core/src/task.rs` (`None` in `Task::new`). Fill it in the three task `SELECT`s of `crates/delilah-daemon/src/store.rs` with `LEFT JOIN workspaces w ON w.task_key = t.key` (qualify the columns), and fix any `Task { … }` literals elsewhere that no longer compile
- [X] T022 [US3] In `app/src/api.ts`, add `branch?: string | null` to `Task` (read-only); in `app/src/components/TaskView.tsx`, show it under the title, monospace, with a `t("Rama")` label, only when present

**Checkpoint**: `cargo test --workspace`, `cd app && npx tsc --noEmit && npx vitest run` green.

---

## Phase 6: Polish & docs

- [X] T023 [P] `docs/status.md`: task branch names built (Claude naming, fallbacks, unique suffix, dialog field, `Task.branch`); remove it from pending if listed
- [X] T024 [P] `README.md`: how a task's branch is named (`feature/<short-name>`, `feature/<KEY>-<short-name>` for Jira), the "Rama" field in the start dialog, `-2` for taken names, `--branch` normalisation
- [X] T025 [P] `docs/architecture.md`: the naming flow (`delilah-core::branch`, `delilah-agents::branch_name`, `delilah-git::free_branch`, `POST /projects/{name}/branch-name`, `start_run`), `Task.branch` from `workspaces.branch`, `DELILAH_CLAUDE_BIN`
- [X] T026 [P] `docs/decisions.md`: Claude (haiku, no tools, temp dir, user settings only) names branches with title-slug and key fallbacks; Jira key kept and `LIBRE-n` dropped; names are never reused (suffix, also in checkout mode, replacing the old switch-to-existing for `--branch`) and never renamed after start
- [ ] T027 Run `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --check`, `cd app && npx tsc --noEmit && npx vitest run`, then quickstart in a preview (`scripts/preview.sh --fresh`)

---

## Dependencies & order

- T001 → T008–T011 (fake claude via `claude_bin`)
- Phase 2: T002→T005, T003→T006, T004→T007; the three pairs are independent of each other
- US1 (Phase 3) needs Phase 2. US2 needs US1's `propose` (T010). US3 needs only T021 and can run in parallel with US2 apart from `app/src/api.ts`, which T016 and T022 share: do them in sequence
- Phase 6 after all stories

## Parallel examples

- Phase 2: T002+T005 (core), T003+T006 (agents), T004+T007 (git) as three parallel packages
- US2/US3: T014+T017+T018 (dialog) alongside T019+T021 (store/core); T022 after T016
- Docs: T023–T026 together

## Implementation strategy

MVP = Phases 1–3. Every started task gets a meaningful, unique `feature/` branch, from the CLI and from the dialog (via the daemon). US2 adds seeing and editing the name, and US3 adds showing it on the task.
