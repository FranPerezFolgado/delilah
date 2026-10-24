# Implementation Plan: Task branch names that describe the task

**Branch**: `feature/LIBRE-6` | **Date**: 2026-10-22 | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `specs/004-task-branch-names/spec.md`

## Summary

When a task starts, its branch is no longer `feature/<KEY>`. It is `feature/` plus
(`<KEY>-` for Jira) a 2–4 word English name that Claude proposes from the title and
notes. The call is a headless haiku one-shot like the submodule suggestion. Pure
rules in `delilah-core::branch` turn any input (Claude's answer, the title, a typed
name) into a valid name; if Claude fails, the title's slug is used, then the key.
`delilah-git::free_branch` adds `-2`, `-3`, … when the name is taken locally or on a
remote, in every repo of the workspace. A new endpoint
`POST /api/v1/projects/{name}/branch-name` gives the start dialog a proposal while
the developer types, shown in a new editable "Rama" field. `start_run` uses the
dialog's name or makes one itself (5 s limit). `Task` gains a read-only `branch`
(from `workspaces.branch`) that the task view shows. No migration. Existing tasks
keep their branch. See [research.md](research.md).

## Technical Context

**Language/Version**: Rust (edition 2024) + TypeScript/React (Tauri app)

**Primary Dependencies**: axum, sqlx (SQLite), tokio, serde_json; the user's `claude` CLI (headless `-p`, haiku). No new crates.

**Storage**: SQLite: no migration; reads the existing `workspaces.branch`

**Testing**: `cargo test -p delilah-core` (slug/compose/custom/fallback), `-p delilah-agents` (prompt, args, parse), `-p delilah-git` (taken local/remote, suffixes, parent submodules), `-p delilah-daemon` (endpoint and `start_run` with a fake `claude` script via `Config::claude_bin`: Claude name, failure → title, explicit name, collision, `Task.branch`); `tsc --noEmit`, vitest (dialog field: proposal, edit kept, sent as `branch`; task view shows branch); manual in a preview

**Target Platform**: macOS (daemon + Tauri app)

**Project Type**: desktop-app + local daemon + CLI

**Performance Goals**: proposal visible in the dialog ≤ 3 s after typing stops in the usual case (10 s limit); starting without a name waits for Claude 5 s at most

**Constraints**: the Claude call must not load project settings/hooks, persist a session or touch the workspace (FR-013); clean env; never reuse an existing branch; never rename a stored branch

**Scale/Scope**: single user, one proposal per typed title (debounced)

## Constitution Check

`.specify/memory/constitution.md` is still the unfilled template; the project's
ground rules (`CLAUDE.md`) serve as gates:

| Gate | Status |
|---|---|
| Nothing from the work repos; fixtures only | ✅ fixture repos + fake `claude` script in tests |
| Agent hooks/settings outside the repo, clean env | ✅ the naming call uses `suggest::env()`, runs in a temp dir with `--setting-sources user` and no tools; writes no settings |
| Try changes only in a preview, never 7420 / pkill | ✅ quickstart uses `scripts/preview.sh`, `DELILAH_CLAUDE_BIN` for the fallback |
| No remote operations without confirming the account | ✅ only reads remote-tracking refs already fetched; no fetch, no push |
| Never edit a shipped migration | ✅ none needed |
| API ↔ client contract kept in sync | ✅ [contracts/api.md](contracts/api.md), additive only |

Post-design re-check: no change.

## Project Structure

### Documentation (this feature)

```text
specs/004-task-branch-names/
├── spec.md
├── plan.md
├── research.md
├── data-model.md
├── quickstart.md
├── contracts/api.md
└── checklists/requirements.md
```

### Source Code

```text
crates/delilah-core/src/branch.rs          # new: slug, title_slug, compose, custom, fallback (+ unit tests)
crates/delilah-core/src/lib.rs             # pub mod branch
crates/delilah-core/src/task.rs            # Task.branch: Option<String> (serde default, skip if none)
crates/delilah-agents/src/branch_name.rs   # new: prompt(title, notes), args(prompt), parse(output) -> short name (+ tests)
crates/delilah-agents/src/lib.rs           # pub mod branch_name
crates/delilah-git/src/workspace.rs        # branch_taken(dir, name), free_branch(dirs, base)
crates/delilah-git/tests/workspace.rs      # local/remote taken, -2/-3, parent submodules
crates/delilah-daemon/src/config.rs        # claude_bin (env DELILAH_CLAUDE_BIN, default "claude")
crates/delilah-daemon/src/branch_name.rs   # new: propose(app, project, key, title, notes, free, submodules, limit) -> Proposal; HTTP handler
crates/delilah-daemon/src/api.rs           # route POST /api/v1/projects/{name}/branch-name
crates/delilah-daemon/src/run.rs           # RunRequest.branch normalised/proposed, free_branch before WorkspaceSpec; doc comment
crates/delilah-daemon/src/store.rs         # LEFT JOIN workspaces → Task.branch in get/list task SELECTs
crates/delilah-cli/src/main.rs             # --branch help text
app/src/api.ts                             # RunRequest.branch, Task.branch, BranchProposal, api.proposeBranch
app/src/components/NewTaskDialog.tsx       # "Rama" field: debounced proposal, edit kept, sent as branch
app/src/components/NewTaskDialog.test.tsx  # field behaviour
app/src/components/TaskView.tsx            # branch under the title
app/src/i18n/en.ts                         # new strings
docs/status.md, docs/architecture.md, docs/decisions.md, README.md
```

**Structure Decision**: follow the existing layers. Pure rules go in `delilah-core`,
the Claude adapter in `delilah-agents` (next to `suggest.rs`), git in `delilah-git`,
and the HTTP handler and orchestration in `delilah-daemon`. The UI keeps its single
start dialog.

## Design notes

- **Order in `start_run`**: after the task and key are known (the `LIBRE-n` key
  exists by then) and before `WorkspaceSpec`, the base name is chosen:
  `req.branch.map(custom)` (400 if `None`), or else `propose(..., 5 s)`. Then
  `free_branch(dirs_of(project, submodules), base)` runs in `spawn_blocking`. Any 400
  happens before `Event::Start` (as the title and project checks do), so a refused
  name leaves a backlog task untouched.
- **`propose`**: Claude (limit) → `compose(key, short, free)`; else `title_slug` →
  `compose`; else `fallback(key, free)`, or `None` when the key isn't known yet (the
  endpoint, for a new free task). Then `free_branch`. Logs the source at info.
- **Claude command**: `Command::new(&app.config.claude_bin)`, `env_clear().envs(suggest::env())`,
  `current_dir(std::env::temp_dir())`, stdin null, `kill_on_drop`, `tokio::time::timeout`.
- **Dialog**: the request is keyed by (project, key, title, free, submodules) and
  debounced 1 s; a stale answer (a newer request went out) is dropped. Once the
  developer edits the field, proposals stop filling it; clearing it lets them back
  in. A non-empty field is sent as `branch`; an empty one lets the daemon decide.
- **Docs**: status (built), README (branch naming and the dialog field),
  architecture (naming flow, `Task.branch`), decisions (Claude names it, with title
  slug and key fallbacks; Jira key kept; never reuse/rename; checkout `--branch`
  reuse dropped).

## Complexity Tracking

None.
