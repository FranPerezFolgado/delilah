# Implementation Plan: Task branches start from a fresh default branch

**Branch**: `feature/LIBRE-12` | **Date**: 2026-10-23 | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `specs/005-fresh-base-branch/spec.md`

## Summary

Before a task branch is created, `delilah-git` fetches `origin` in every repo the
task starts from (bounded by one shared 30 s deadline, never prompting, one fetch
per repo at a time), resolves the remote's default branch (`origin/HEAD` →
`origin/main` → `origin/master`, falling back to local `main`/`master`/`HEAD`) and
creates the branch from that **commit sha** (no upstream tracking). `create_workspace`
and `use_checkout` return, per repo, the base used and how the fetch went.
`run.rs::start_run` logs every base and, for each failed fetch, records a
`base-stale` notice in `pr_notices`, which the Bandeja lists as a new non-blocking
item kind and the task screen shows as a dismissable banner. Existing branches in
checkout mode are left untouched; every other failure stays as today. See
[research.md](research.md).

## Technical Context

**Language/Version**: Rust 2024 (daemon, git crate); TypeScript + React (`app/`)

**Primary Dependencies**: git CLI (2.38+), std::process; axum, sqlx, tokio; React, vitest + Testing Library. No new crates.

**Storage**: no migration; new `kind = 'base-stale'` rows in `pr_notices` (0011)

**Testing**: `cargo test -p delilah-git` (fixture from `scripts/make-fixture-parent.sh`: bare remotes advanced from a second clone, unreachable `origin`, hanging `core.sshCommand` for the timeout); `cargo test -p delilah-daemon` (start with a failed fetch → notice + inbox item; dismiss); `cd app && npx tsc --noEmit && npx vitest run` (`InboxView.test.tsx`, `TaskScreen.test.tsx`)

**Target Platform**: macOS (daemon via launchd, no tty); tried in a preview (`scripts/preview.sh`)

**Project Type**: desktop app with a local daemon (API-first)

**Performance Goals**: online start + one fetch per repo; offline start ≤ +30 s total (SC-003/004)

**Constraints**: never move local branches other than the task's (FR-006); never prompt (FR-008); git calls keep `core.hooksPath=/dev/null`, `core.fsmonitor=false`

**Scale/Scope**: one new git module, changes to two git functions and `start_run`, one inbox kind, one banner

## Constitution Check

`.specify/memory/constitution.md` is still the unfilled template; the ground rules
in `CLAUDE.md` serve as gates:

| Gate | Status |
|---|---|
| Nothing from the work repos; fixtures only | ✅ tests use the fixture script and temp dirs |
| Agent hooks/settings outside the repo, clean env | ✅ untouched |
| Try changes only in a preview, never 7420 / pkill | ✅ [quickstart.md](quickstart.md) uses `scripts/preview.sh` |
| Remote operations: confirm the account | ✅ only a read-only `fetch` of the project's own `origin`, as archive-on-merge already does; no push |
| Migrations: never edit a shipped one | ✅ no migration; 0011's comment left as is |
| Contracts (`api.rs`/`inbox.rs` ↔ `api.ts`) | ⚠️ additive: new inbox kind + optional `notice` field ([contracts/api.md](contracts/api.md)); contract review in the phase that touches them |

Post-design re-check: no change.

## Project Structure

### Documentation (this feature)

```text
specs/005-fresh-base-branch/
├── spec.md, plan.md, research.md, data-model.md, quickstart.md
├── contracts/api.md
└── checklists/ (requirements.md, git.md)
```

### Source Code

```text
crates/delilah-git/src/
├── command.rs     # + git_with_timeout (GIT_TERMINAL_PROMPT=0, kill at deadline), GitError::Timeout, Deadline
├── base.rs        # NEW: fetch lock per repo, fetch_fresh, resolve_base, RepoBase, FetchOutcome
├── workspace.rs   # create_workspace / init_submodules / use_checkout start from the resolved sha; fetch() takes the lock
└── lib.rs         # re-exports
crates/delilah-git/tests/
├── workspace.rs   # adapt to new return types
└── base.rs        # NEW: stale main, parent submodules, offline, timeout, existing branch, no upstream, concurrency
crates/delilah-daemon/src/
├── run.rs         # Deadline, log bases, record base-stale notices after a successful start
├── inbox.rs       # ItemKind::BaseStale, InboxItem.notice, list base-stale notices without a PR
└── actions.rs     # test helper `started()` adapted to new return types
app/src/
├── api.ts                     # InboxKind += "base-stale", InboxItem.notice?
├── alerts.ts                  # SAYS["base-stale"]
├── components/InboxView.tsx   # KIND entry, "Descartar" → api.dismissNotice
├── components/TaskScreen.tsx  # banner of the task's base-stale items
├── App.tsx                    # pass the task's base-stale items to TaskScreen
└── i18n/en.ts                 # English strings
docs/ architecture.md (Workspaces), decisions.md (new entry), status.md; README.md
```

**Structure Decision**: the git work lives in `delilah-git` (new `base.rs`), as all
git does; the daemon only builds the deadline, logs, and turns failures into
notices; the UI reuses the notice dismiss route.

## Design

1. **`command.rs`** — `pub struct Deadline(Instant)` (`after(Duration)`,
   `remaining()`); `git_with_timeout(dir, args, opts, deadline)`: same `command()`
   plus `GIT_TERMINAL_PROMPT=0`, stdin null, piped out/err, `try_wait` every 50 ms,
   `kill` + `wait` past the deadline → `GitError::Timeout { dir, args }`.
2. **`base.rs`**
   - `fn has_origin(dir)` (`remote get-url origin`).
   - `fn fetch_fresh(dir, opts, deadline) -> FetchOutcome`: no origin → `NoRemote`;
     take the repo's lock (`try_lock` polling until deadline, else `Failed("busy")`);
     skip if the same repo was fetched < 5 s ago (R5); `fetch --quiet --no-tags origin`
     with the deadline; on success, if `origin/HEAD` is missing,
     `remote set-head origin --auto` within the deadline (failure ignored).
   - `fn resolve_base(dir, opts) -> (refname, sha)`: first of `origin/HEAD`
     (reported by the name it points to, e.g. `origin/main`), `origin/main`,
     `origin/master`, `main`, `master`, `HEAD` that resolves (`rev-parse --verify --quiet <ref>^{commit}`).
   - `pub fn prepare_base(dir, label, opts, deadline) -> RepoBase` = fetch + resolve.
   - `fetch()` in `workspace.rs` takes the same lock (no deadline: blocking lock).
3. **`workspace.rs`**
   - `create_workspace(root, spec, opts, deadline)`: Repo → `prepare_base(root, ".")`,
     `worktree add -b <branch> <path> <sha>`. Parent → `prepare_base(root, ".")`,
     `worktree add --detach <path> <sha>`, `submodule update --init -- <chosen>`, then
     per submodule `prepare_base(ws/sub, sub)` and `checkout -q -b <branch> <sha>`.
     Returns the bases.
   - `use_checkout(root, spec, opts, deadline)`: per folder (repo `.` or each chosen
     submodule; never the parent folder): if the branch exists → `checkout` as today,
     `RepoBase{fetch: Existing}`; else `prepare_base` then `checkout -q -b <branch> <sha>`.
     The dirty check stays first (before any fetch); the rollback stays.
4. **`run.rs::start_run`** — `Deadline::after(30 s)` built before `spawn_blocking`;
   on success, `tracing::info!` per base (FR-010) and, for each `Failed(reason)`,
   `insert_notice` (`base-stale`, cause `start`, text via `app.lang().pick`, detail
   JSON), then `app.emit(Topic::Inbox)`. Notice failures are logged, never fail the start.
5. **`inbox.rs`** — `ItemKind::BaseStale`; in the notices loop, `base-stale` is
   handled before the PR lookup: skip if the task is terminal without workspace,
   else an item with `notice: Some(id)`, `pr: None`. `InboxItem.notice:
   Option<i64>` (`skip_serializing_if`).
6. **UI** — `InboxView`: `"base-stale": { label: "Rama posiblemente atrasada",
   icon: "warning", cls: "c-warn" }` (use the existing warning style/icon if
   there is one), text shown as is, "Descartar" button. `TaskScreen`: new prop
   `notices: InboxItem[]`, a banner above the tabs per item with "Descartar".
   `alerts.ts`: `SAYS` entry; not added to `BLOCKING`.

## Phases (for tasks)

1. **Git core** (delilah-git): `Deadline`, `git_with_timeout`, `base.rs`, fetch lock;
   tests for resolve/fetch/timeout/no-remote/concurrency.
2. **Workspaces** (delilah-git): `create_workspace`, `init_submodules`,
   `use_checkout` from the fresh sha; tests for US1–US3 + existing branch + no upstream.
3. **Daemon** (`run.rs`, `inbox.rs`, `actions.rs` helper): deadline, logs,
   `base-stale` notices, inbox item; tests (failed fetch → item; dismiss resolves it;
   archive resolves it).
4. **UI**: `api.ts`, `alerts.ts`, `InboxView`, `TaskScreen`, `App`, i18n; tests.
5. **Docs + version**: architecture (Workspaces), decisions (new entry: base =
   remote default, sha start point, warn-and-continue, no fast-forward), status,
   README; bump 0.4.0 → 0.5.0 (minor).

## Complexity Tracking

No gate violations to justify.
