# Delilah

Native macOS orchestrator for coding agents, written in Rust. Design:
[docs/vision.md](docs/vision.md). UI screenshots: `docs/redesign/`.

## Current scope

Daemon + CLI and the desktop app (Tauri, `app/`), used daily; only the Claude Code
adapter. MVP-0 and MVP-1 of the design doc are done; what is built and what comes
next is in [docs/status.md](docs/status.md).

## Start here

Read these before changing anything; keep them true when you change behaviour:

- [docs/status.md](docs/status.md): what is built, what is pending, known problems.
- [docs/architecture.md](docs/architecture.md): crates, daemon modules, data, task
  states and phases, hooks, workspaces, the UI.
- [docs/decisions.md](docs/decisions.md): what was decided and why (including what
  was ruled out).
- [docs/development.md](docs/development.md): running, testing, conventions,
  releasing, gotchas.
- [docs/vision.md](docs/vision.md): the original plan, for what is not built yet.
- [docs/usage.md](docs/usage.md): every feature as the user sees it (the README is a short newcomer page).

When a feature lands, update `docs/status.md` (and docs/usage.md for user-facing
behaviour); when a decision is taken, add it to `docs/decisions.md`.

## Versioning: every Spec Kit task bumps the version

Every feature or fix done with Spec Kit (`/feature`, `/orchestrate`,
`/speckit-implement`) ends with a version bump on its branch, before the PR.
Decide the level yourself from what the change does (semver):

- **major**: breaks something already in use — HTTP API or CLI changes that old
  clients can't handle, data/state that needs manual migration, removed features.
- **minor**: a new user-facing feature or capability, backwards compatible.
- **patch**: fixes, docs, internal refactors with no new behaviour.

Bump from the version on `main` (re-check it if `main` moved before merging),
in the four places listed in [docs/development.md](docs/development.md#releasing)
plus the lockfiles (`scripts/bump-version.sh <level> --commit` does both), as its own commit
`chore: bump version to X.Y.Z`, and say in the PR which level you picked and why. Tagging,
building and publishing are `make release` on `main` and the tag workflow, as in the release steps.

## Ground rules

- Never copy code or data from private repos; use `scripts/make-fixture-parent.sh` and
  the Jira JSON fixtures.
- Agent hooks go in a settings file outside the repo, passed with `claude --settings`;
  never in the worktree or the global `~/.claude`. Spawn agents with a clean env.
- If several GitHub accounts are configured, confirm the one `gh` uses before any
  remote operation.
- Submodules over `file://` need `git -c protocol.file.allow=always`.

## Parallelize by default

The setup is built for parallel work, so use it without being asked. Whenever a task
has pieces that don't depend on each other, run them at the same time:

- **Reading, research, reviews, checks:** subagents (`code-researcher`, the
  reviewers, `ui-verifier`, `doc-searcher`), all launched in one message.
- **Independent pieces of code** (separate files, neither needs the other's result):
  one worktree each, since each worktree gets its own preview slot (1–9). With Spec
  Kit, `/orchestrate` already runs packages that share no files in parallel.
- **Tasks of their own** (a separate fix or feature that turns up along the way):
  propose them as a separate session or Delilah task instead of chaining them here.

Don't parallelize what a handful of tool calls settles, two writers on the same
files, or steps that need each other's output. When a plan has independent steps
and you run them one after another anyway, say why in one line.

## Stack & checks

| Area | Where | Check |
|---|---|---|
| Rust workspace | `crates/*` | `cargo test -p <crate>`; everything: `cargo test --workspace` |
| Desktop shell | `app/src-tauri` (outside the workspace) | `cargo test --manifest-path app/src-tauri/Cargo.toml` |
| Web UI | `app/` (React + TS, Vite) | `cd app && npx tsc --noEmit && npx vitest related --run <files>`; everything: `npx vitest run` |

- Migrations: `crates/delilah-daemon/migrations/NNNN_*.sql` (SQLite, sqlx; never edit a shipped one).
- Contracts: the daemon's HTTP API in `crates/delilah-daemon/src/api.rs` ↔ its client types in `app/src/api.ts`.

## QMD Collection

Active collections: `delilah` (docs/), `delilah-specs` (specs/)

## Trying changes: always in a preview

Delilah is built with Delilah: the installed app (daemon on 7420, `~/.delilah`)
runs the agents working on this repo, so never restart, stop or `pkill` any
`delilahd` to try a change, and never use 7420 or `scripts/dev-daemon.sh` for it.
Try changes in a preview of the branch instead:

```bash
scripts/preview.sh            # this worktree's daemon + UI, in the first free slot
scripts/preview.sh --fresh    # from an empty state
```

It prints the UI URL (`http://localhost:150N`) and the API (`http://127.0.0.1:750N`),
with its own state in `fixtures/preview-home-N` and the fixture repos to register
as projects. Open the UI in a browser (or the browser pane) to try the feature;
use `DELILAH_URL=<API> target/debug/delilah …` or curl for the API. Run it in the
background and stop it with Ctrl-C (or by stopping that process), never by name.
A `PreToolUse` hook (`.claude/hooks/guard-installed-daemon.sh`) blocks the commands that would
touch the installed app; if it blocks you, use a preview or ask the user.
Only what is native to macOS (notifications, menu, Dock badge, global ⌘P) needs
the Tauri app; say so instead of trying it.
