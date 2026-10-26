# Implementation Plan: Task name and branch from the first analysis

**Branch**: `feature/LIBRE-11` | **Date**: 2026-10-23 | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `specs/005-task-name-from-spec/spec.md`

## Summary

A free task starts without a branch: its worktree is detached (repo) or its submodules
are left detached (parent), and in checkout mode nothing is switched. Its folder is
still `workspaces/<KEY>`. A new daemon module, `branching.rs`, settles the branch with
one idempotent function, `ensure`:
1. If Spec Kit's `before_specify` hook created a branch, Delilah adopts it.
2. Otherwise, if a new spec folder has appeared, Delilah creates the branch with its
   name.
3. Otherwise, when the guard calls it, Delilah creates a branch from the title's slug.

It then applies the branch to a parent's submodules and records it. `ensure` runs:
- on the hooks of a branchless task's sessions;
- on turn end;
- on the scheduler tick and at daemon start;
- from a retry endpoint.

A PreToolUse **guard**, registered only for branchless sessions, calls it before a
code edit or a commit/push, so nothing lands on the base branch. On the same triggers,
the first spec's title replaces the task's title once (`title_source`), keeping
`previous_title` for "Restaurar". Titles can now be edited on running tasks. Failures
go to `tasks.branch_error` and a Bandeja item with a retry. See
[research.md](research.md).

## Technical Context

**Language/Version**: Rust 2024 (daemon, core, git, agents); TypeScript + React (`app/`)

**Primary Dependencies**: axum, sqlx (SQLite), tokio; git CLI via `delilah-git`; React, Vite; vitest + Testing Library

**Storage**: SQLite: migration `0015_task_branch_from_spec.sql`:
- `tasks.title_source`, `tasks.previous_title`, `tasks.branch_error`
- `workspaces.spec_dirs_at_start`
- `workspaces.branch = ''` means no branch yet

**Testing**:
- `cargo test -p delilah-git`: temp repos and a parent with a submodule
- `cargo test -p delilah-daemon`: `branching`, the guard, the title, PR refusal, the migration
- `cargo test -p delilah-agents`: guard hook in the settings
- `cd app && npx tsc --noEmit && npx vitest run`: TaskScreen header, InboxView `branch` item

**Target Platform**: macOS; tried in a preview (`scripts/preview.sh`)

**Project Type**: desktop app with a local daemon (API-first)

**Performance Goals**:
- Title and branch within 10 s of the spec being written (SC-001). In practice it's
  the same tool call's PostToolUse.
- The guard adds one local HTTP round trip per tool call, and only while the task is
  branchless.

**Constraints**:
- Jira tasks and tasks that already exist are unchanged (FR-013/014).
- The running session is never moved or restarted (FR-009).
- Agent settings stay outside the repo.
- Nothing is ever committed onto the base branch (SC-003).

**Scale/Scope**:
- One new daemon module and one migration.
- Two endpoints (retry, guard) and one inbox kind.
- Changes to `delilah-git` workspace creation, `run.rs`, `specs.rs`, `pr_api/open.rs`,
  `archive.rs`, the agent settings, `PATCH` tasks, TaskScreen and InboxView.

## Constitution Check

`.specify/memory/constitution.md` is still the unfilled template; the project's ground
rules (`CLAUDE.md`) serve as gates:

| Gate | Status |
|---|---|
| Nothing from the work repos; fixtures only | ✅ temp repos in tests; fixture projects in the preview |
| Agent hooks/settings outside the repo, clean env | ✅ the guard is one more hook in the per-session `--settings` file; no git hooks in the repo |
| Try changes only in a preview, never 7420 / pkill | ✅ [quickstart.md](quickstart.md) |
| No remote operations without confirming the account | ✅ no new push; PR refused when branchless |
| Migrations: never edit a shipped one | ✅ new `0015` |
| Contracts (`api.rs` ↔ `api.ts`) | ⚠️ changed, additively: [contracts/api.md](contracts/api.md); contract review in the phase that touches them |
| Decision 17 ("files and signals only move forward"), decision 15 ("Delilah follows the repo, doesn't drive Spec Kit") | ✅ Delilah reads the branch and spec Spec Kit left; it creates a branch only where Spec Kit didn't. New decision entry in `docs/decisions.md` |
| Version bump (CLAUDE.md) | **minor**: new user-facing behaviour, backwards compatible |

Post-design re-check: no change.

## Project Structure

### Documentation (this feature)

```text
specs/005-task-name-from-spec/
├── spec.md
├── plan.md              # this file
├── research.md          # R1–R9 (answers checklists/workspace.md)
├── data-model.md
├── quickstart.md
├── contracts/api.md
├── checklists/{requirements,workspace}.md
└── tasks.md             # /speckit-tasks
```

### Source Code (repository root)

```text
crates/delilah-git/src/workspace.rs        # WorkspaceSpec.branch: Option; detached repo worktree;
                                           # submodules without checkout -b; use_checkout without switch
                                           # (+ parent "." in previous); new apply_branch / current_branch /
                                           # branch_exists helpers
crates/delilah-core/src/task.rs            # TitleSource, previous_title, branch_error
crates/delilah-daemon/migrations/0015_task_branch_from_spec.sql
crates/delilah-daemon/src/branching.rs     # NEW: ensure(), spec-folder scan, slug, title from spec,
                                           # guard handler, retry handler, per-task lock
crates/delilah-daemon/src/run.rs           # free task → branchless WorkspaceSpec, spec_dirs_at_start,
                                           # title_source initial, prompt sentence (R8)
crates/delilah-daemon/src/store.rs         # set_title (any non-archived task), title_source/previous_title,
                                           # branch_error
crates/delilah-daemon/src/store_runs.rs    # spec_dirs_at_start, set_workspace_branch
crates/delilah-daemon/src/phases.rs        # on_hook / turn end → branching::on_activity
crates/delilah-daemon/src/scheduler.rs     # tick → branching::recover
crates/delilah-daemon/src/hooks.rs + api.rs  # /hooks/claude/guard, POST /tasks/{key}/branch, PATCH title
crates/delilah-daemon/src/specs.rs         # branchless: skip branch match and changed_paths
crates/delilah-daemon/src/pr_api/open.rs, pr_loop/mod.rs  # refuse / skip branchless
crates/delilah-daemon/src/archive.rs       # branchless archive & delete
crates/delilah-daemon/src/inbox.rs         # ItemKind::Branch
crates/delilah-agents/src/claude.rs        # settings(): optional guard PreToolUse hook
app/src/api.ts                             # Task fields, retryBranch(), InboxItem kind
app/src/components/TaskScreen.tsx          # editable title, "Antes: …" + Restaurar
app/src/components/InboxView.tsx           # branch item + Reintentar
app/src/i18n/en.ts                         # new strings
docs/{status,architecture,decisions}.md, README.md
```

**Structure Decision**: one new daemon module (`branching.rs`) owns the rules. The
other modules only call `ensure` / `recover` or treat `has_branch() == false`.

## Complexity Tracking

| Addition | Why needed | Simpler alternative rejected because |
|---|---|---|
| PreToolUse guard hook | The only point before a commit where Delilah can create the branch (SC-003) | PostToolUse only: a commit in the same turn could land on the base branch first |
| `spec_dirs_at_start` | To tell this task's analysis from old features' folders | Comparing with the branch's changes: there is no branch yet |
