# Implementation Plan: Parent tasks and subtasks (integration branch)

**Branch**: `012-parent-subtasks` | **Date**: 2026-10-27 | **Spec**: [spec.md](spec.md)
**Input**: Feature specification from `specs/012-parent-subtasks/spec.md`

## Summary

Tasks gain a kind: **normal** (today), **parent task** (an integration branch with a draft PR
against the default branch, no agent at start, merge commit by default) and **subtask** (its
own worktree branched from the parent task's branch, PR against it, squash). Kind and link
are two new task columns; everything else (live, merged, dropped, progress) is derived from
existing data on read. One daemon module (`integration.rs`) answers "what is this task's
base", and the workspace, PR-open, base-status/update-branch, merge, archive and delete paths
ask it. The UI groups subtasks under their parent task and shows "N/M subtasks merged".
Core only: the Jira adapter is a later spec.

## Technical Context

**Language/Version**: Rust 2021 (workspace), TypeScript + React (Vite) in `app/`
**Primary Dependencies**: axum, sqlx (SQLite), serde, tokio; `git` and `gh` CLIs; React, vitest
**Storage**: SQLite `tasks` table (+2 columns, migration 0025); `projects.settings` JSON (+1 field)
**Testing**: `cargo test -p delilah-core -p delilah-git -p delilah-daemon -p delilah-cli`;
`npx vitest`; daemon e2e with the fake Claude and fake `gh` (`crates/delilah-daemon/tests`)
**Target Platform**: macOS (daemon + Tauri app); nothing native changes
**Project Type**: desktop app with a local daemon (HTTP API) and CLI
**Performance Goals**: progress for a parent task with ≤ 50 subtasks computed in one query
(tasks by `parent_task`) + one PR query, no per-child round trips; SC-006 within one PR poll
**Constraints**: additive API (FR-029); never touch the user's checkout for these kinds;
never merge or mark ready on its own (FR-020); `ProjectKind::Parent` keeps its meaning
**Scale/Scope**: 1 core module, 1 migration, 1 new daemon module, ~10 daemon files, 4 new
endpoints, 4 CLI commands/flags, ~7 UI components

## Constitution Check

`.specify/memory/constitution.md` is the unfilled template; the project's rules are in
`CLAUDE.md` and `docs/decisions.md`. Gates:

| Gate | Status |
|---|---|
| Never edit a shipped migration | ✅ new `0025_task_hierarchy.sql` |
| API ↔ `app/src/api.ts` in sync, additive | ✅ [contracts/api.md](contracts/api.md) |
| Settings kept by Delilah, not in the repo (decision 50) | ✅ `projects.settings` |
| Fresh base branch (decision 66) | ✅ same bounded fetch, base ref swapped for subtasks (R6) |
| Hooks outside the worktree, clean env | ✅ untouched (parent tasks launch no agent) |
| No private-repo data; fixtures only | ✅ fixture repos + fake `gh` |
| Try in a preview, never on 7420 | ✅ [quickstart.md](quickstart.md) |
| Docs updated (status, usage, decisions, architecture) | ✅ phase 9 |
| No version bump | ✅ none planned |

Post-design re-check: no violations; nothing to justify beyond Complexity Tracking.

## Design

See [research.md](research.md) (R1–R14), [data-model.md](data-model.md),
[contracts/api.md](contracts/api.md), [quickstart.md](quickstart.md).

Naming: in code and docs it is always **parent task** (`TaskKind::ParentTask`,
`parent_task`), never bare "parent", which already means a parent repository
(`ProjectKind::Parent`) — R1.

### Core (`crates/delilah-core`)

- New `src/hierarchy.rs` (pure, unit-tested): `TaskKind`, `Progress`, `ChildFacts { merged,
  archived }`, `progress(&[ChildFacts])`, `is_live`, and the checks returning a `Refusal`
  enum: `check_new_subtask(parent: Option<&Task>, parent_archived)`,
  `check_make_parent_task(task, has_open_pr, has_session)`, `check_attach(task, parent,
  started)`, `check_start_subtask(parent_has_workspace)`. `Refusal` maps 1:1 to the reasons in
  the contract.
- `task.rs`: `Task.kind`, `Task.parent_task` (serde defaults).
- `project.rs`: `MergePreference`, `ProjectSettings.parent_task_merge`.

### Git (`crates/delilah-git`)

- `WorkspaceSpec.base: Option<String>`; `prepare_base` resolves only
  `refs/remotes/origin/<base>` when set; `GitError::BaseMissing` (R6).
- `commit_empty(dir, message)` (R11).
- `merge_refs(dir, &[ref])` for update-branch with several refs, or extend the existing merge
  helper used by `base_update::merge_all` to take a list (R9).

### Daemon (`crates/delilah-daemon`)

- `migrations/0025_task_hierarchy.sql`; `store.rs`: columns in INSERT/SELECT/`task_from_row`,
  `subtasks_of(key)`, `set_kind(key, kind, parent_task)`.
- New `integration.rs`: `base_for(app, &Task) -> Base`, `subtask_facts(app, key) ->
  Vec<(Task, ChildFacts)>`, `progress(app, key)`, `live_subtasks(app, key)`,
  `preferred_method(kind, settings, allowed)`.
- `api.rs`: `NewTask.kind/parent_task` + validation in `build_task_with` (project inherited,
  parent task stored with workflow none and toggles off — R4/R7); `TaskDto` (not
  `TaskView`, which exists in core) with `subtask_progress` for GET task/list. The new routes
  (`subtasks`, `make-parent-task`, `attach`, `push-branch`) live in a new `api_hierarchy.rs`,
  not in `api.rs` (2.4k lines); `prs/{id}/ready` in `pr_api`.
- `run.rs` (5.1k lines): only the calls; the kind rules (forced worktree, base, submodules,
  skip primary session) are functions in `integration.rs`. Kind forces `in_checkout = false` and no branchless start (R5); subtask start
  checks the parent task's workspace and passes `WorkspaceSpec.base` and the parent task's
  submodules (R6/R7); parent task: after `WorkspaceReady`, push the branch and skip the
  primary session (R4). `branching.rs`: `starts_branchless` false for both kinds; spec-name
  adoption skips them.
- `pr_api/open.rs`: every base call site goes through `integration::base_for` (`base_of` at
  :102 and :236, the literal `--base` at :467), so autopilot, forward and hand-off PRs get it
  too; a test asserts no PR path uses the default branch for a subtask (SC-002).
  `pr_api/dto.rs`: `base_branch` and `expected_base`; `PrView` warns when a subtask PR
  targets another branch, naming it (FR-013; Delilah never retargets); draft forced for a parent task unless
  progress is ready; `empty_commit` path (R11). `pr_api/merge.rs`: `preferred_method`,
  `confirm_live_subtasks` (R10/R12); new `ready` handler. `pr_api/dto.rs`: `method`.
  `gh/mod.rs`: `pr_ready`.
- `base_update.rs`: refs to merge from `base_for` (R9); no second resolver: the integration
  ref is a parameter of the shared `delilah-git` base helper, used by both `prepare_base` and
  `base_update`; `subtask_commits`, `base` in the status.
- `actions.rs` (archive, delete) and `pr_loop/merge.rs::archive_merged`: `live_subtasks`
  refusal / done-not-archived + notice (R13). `pr_loop`: emit `Topic::Tasks` for the parent
  task when a subtask's PR merges, and the one-time `parent-task-ready` notice (FR-023).
- `integration.rs` rule: no `gh` or git calls; its IO is store queries (`subtasks_of` + PR
  rows), so the modules that call it don't pick up git/gh coupling through it.
- Push again (spec edge case "integration branch not on the remote"): `POST
  /tasks/{key}/push-branch` for a parent task (existing `push_branch`), shown by `TaskHeader`
  and in the `parent-task-branch-missing` refusal of a subtask start.
- Agent-less `Running` (R4) is verified in phase 4: startup recovery, idle/stale detection and
  extra-session ending must not move a parent task; test: restart leaves it `Running`.
- Delete path: `parent_task` links of archived subtasks are cleared when their parent task is
  deleted, so a reused `LIBRE-N` key never inherits orphans.
- `inbox.rs`: kinds `parent-task-live`, `parent-task-ready`. `lang.rs`: messages for every
  `Refusal` (the core returns only the enum; reason strings and text stay in the daemon).

### CLI (`crates/delilah-cli`)

`new --parent-task | --parent <KEY>`, `make-parent-task`, `attach --parent`, tree in `ls`,
kind/parent/progress/subtasks in `show` (contracts §CLI).

### UI (`app/`)

- `api.ts`: types and calls from the contract; `DEFAULT_PROJECT_SETTINGS.parent_task_merge`.
- `Sidebar.tsx`: subtasks nested under their parent task (collapsible), "N/M" badge.
- `TaskHeader.tsx` / `TaskScreen.tsx`: parent task shows progress instead of "Running", the
  subtask list (key, title, state, PR state, dropped), "New subtask", the ready suggestion;
  subtask shows a link to its parent task.
- `NewTaskDialog.tsx`: kind choice (normal / parent task) and, opened from a parent task,
  creates a subtask; workspace choice hidden for both kinds; agent options hidden for a
  parent task.
- `BacklogTaskView.tsx`: "Make parent task" and "Attach to parent task…" with refusal
  reasons.
- `PrView.tsx`: shows `pr.method`; draft badge and "Mark ready"; empty-commit offer on
  `no-commits` for a parent task; confirmation on `live-subtasks`; base-status banner with
  `subtask_commits`.
- `ProjectSettings.tsx`: "Parent task merge method".
- i18n: Spanish keys in place, English in `src/i18n/en/`.

## Project Structure

### Documentation (this feature)

```text
specs/012-parent-subtasks/
├── spec.md, plan.md, research.md, data-model.md, quickstart.md
├── contracts/api.md
├── checklists/requirements.md, checklists/flows.md
└── tasks.md            # /speckit-tasks
```

### Source Code (repository root)

```text
crates/delilah-core/src/{hierarchy.rs (new), task.rs, project.rs, lib.rs}
crates/delilah-git/src/{workspace.rs, base.rs, lib.rs}
crates/delilah-daemon/migrations/0025_task_hierarchy.sql (new)
crates/delilah-daemon/src/{integration.rs (new), api_hierarchy.rs (new), api.rs, store.rs, run.rs, branching.rs,
                           base_update.rs, actions.rs, inbox.rs, lang.rs, gh/mod.rs,
                           pr_api/{open.rs, merge.rs, dto.rs}, pr_loop/{mod.rs, merge.rs}}
crates/delilah-daemon/tests/  (e2e: subtask base, PR base, archive refusal)
crates/delilah-cli/src/main.rs
app/src/{api.ts, i18n/en/*}
app/src/components/{Sidebar.tsx, TaskHeader.tsx, TaskScreen.tsx, NewTaskDialog.tsx,
                    BacklogTaskView.tsx, PrView.tsx, ProjectSettings.tsx, SubtaskList.tsx (new)}
docs/{status.md, usage.md, decisions.md, architecture.md}
```

**Structure Decision**: existing layout; one pure core module for the rules, one daemon
module that owns "base of a task" and the hierarchy queries, one new UI component.

## Phases (for tasks)

1. **Core model**: `hierarchy.rs`, task fields, merge setting (tests first).
2. **Storage**: migration 0025, store read/write, `subtasks_of`, migration test on a
   pre-upgrade DB.
3. **Git**: `WorkspaceSpec.base`, `BaseMissing`, `commit_empty`, multi-ref merge (tests on
   fixture repos).
4. **Create & start (US1)**: `integration.rs` (base_for, progress), `api.rs` create/views,
   `run.rs` + `branching.rs`, make-parent-task/attach endpoints.
5. **PR flows (US2/US3)**: open (base, draft, empty commit), merge (method, confirmation),
   ready, base-status/update-branch, PR target warning, PR-loop events and ready notice.
6. **Archive/delete (US4)**: actions + `archive_merged` + notice.
7. **CLI**.
8. **UI**: api.ts, sidebar tree, task screen/header, subtask list, new-task dialog, backlog
   actions, PrView, project settings, i18n.
9. **Docs**: decision (task kinds, integration branch, no stacked PRs), usage, status,
   architecture.

Phases 1–3 are the foundation (1 first; 2 and 3 in parallel: different crates). 4 needs
1–3. 5 and 6 need 4 and touch separate files, so they run in parallel; 7 needs only the
contract and 4. 8 needs the contract only and can run in parallel with 5–7. 9 last.

## Complexity Tracking

| Choice | Why | Simpler alternative rejected because |
|---|---|---|
| Kind column + derived state | no new status, no stored counters to race (CHK021) | a `Open` status or a stored progress counter touch every status match and race on concurrent merges |
| `integration::base_for` used by four paths | one rule for the subtask base | patching each of the three existing base resolvers scatters the rule |
| Separate convert/attach endpoints | own preconditions and refusals (FR-008) | PATCH `BacklogChange` mixes edit and lifecycle checks |
| Empty-commit offer | GitHub refuses a PR with no diff | opening the parent task PR only after the first subtask merges hides the draft PR the user asked for |
