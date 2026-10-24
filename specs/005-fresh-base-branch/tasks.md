# Tasks: Task branches start from a fresh default branch

**Input**: Design documents from `specs/005-fresh-base-branch/`
**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/api.md, quickstart.md

**Tests**: test-first. Write each test task before its implementation and see it fail.

**Checks**: `cargo test -p delilah-git`, `cargo test -p delilah-daemon`; `cd app && npx tsc --noEmit && npx vitest related --run <files>`; at the end `cargo test --workspace`, `cargo test --manifest-path app/src-tauri/Cargo.toml` and `npx vitest run`.

## Format: `[ID] [P?] [Story] Description`

## Phase 1: Setup

**Purpose**: test helpers shared by every story. No new dependencies.

- [X] T001 Add helpers to the test fixture in `crates/delilah-git/tests/workspace.rs` (or a shared `crates/delilah-git/tests/common/mod.rs` used by both test files): `advance_remote(remote: &Path, file, msg) -> String` (clones the bare remote into a temp dir, commits, pushes `main`, returns the new sha); `remote_head(remote) -> String` (`git -C <bare> rev-parse main`); `upstream_of(dir, branch) -> Option<String>` (`git rev-parse --abbrev-ref <branch>@{u}`, `None` on failure). Use the existing `git()` helper and `Fixture` (bare remotes in `env/remotes/*.git`).

---

## Phase 2: Foundational (blocks every story)

**Purpose**: bounded, non-prompting git calls; fetch per repo with a lock; base resolution. No caller changes yet.

- [X] T002 Write failing tests in `crates/delilah-git/src/command.rs` (tests module): `Deadline::after(Duration)` / `remaining()` saturates at zero; `git_with_timeout` returns the output like `git` for a fast command (`rev-parse --show-toplevel` in a temp repo); a fetch from `ssh://example.invalid/x` with `-c core.sshCommand="sleep 30"` (pass it via a test-only extra config, or a repo whose local config sets `core.sshCommand`) and a 1 s deadline returns `GitError::Timeout` in under 3 s and leaves no child running; an already-expired deadline returns `Timeout` without spawning.
- [X] T003 Implement T002 in `crates/delilah-git/src/command.rs`: `pub struct Deadline(Instant)` with `after`, `remaining`, `expired`; `pub(crate) fn git_with_timeout(dir, args, opts, deadline) -> Result<String, GitError>` built on `command()`, adding env `GIT_TERMINAL_PROMPT=0`, stdin null, stdout/stderr piped, polling `try_wait` every 50 ms, `kill()` + `wait()` past the deadline; `GitError::Timeout { dir, args }` ("`git {args}` in {dir} took too long"). Re-export `Deadline` from `crates/delilah-git/src/lib.rs`.
- [X] T004 Write failing tests in new `crates/delilah-git/tests/base.rs` for `prepare_base(dir, label, opts, deadline) -> RepoBase` on a clone of a fixture remote: (a) after `advance_remote`, the base is `refname == "origin/main"` and `sha` == the remote's new head, `fetch == Fetched`, and local `main` has not moved; (b) a repo with no `origin` (`git init` + one commit) gives `NoRemote`, `refname == "main"` and its head; (c) `origin` set to `/nonexistent` gives `Failed(reason)` with a non-empty reason and falls back to the last fetched `origin/main` (sha before the advance); (d) a repo whose `origin/HEAD` was deleted (`git remote set-head origin -d`) still resolves `origin/main` and, after a successful fetch, `origin/HEAD` is set again; (e) a remote whose default is `master` (create a bare repo with `-b master`) resolves `origin/master`; (f) `origin/HEAD` is reported by the name it points to (`origin/main`, not `origin/HEAD`); (g) two threads calling `prepare_base` on the same repo at once both return `Fetched` (or one reuses the other's fetch) and neither `Failed`.
- [X] T005 Implement T004 in new `crates/delilah-git/src/base.rs`: `pub enum FetchOutcome { Fetched, NoRemote, Failed(String), Existing }`; `pub struct RepoBase { repo, refname, sha, fetch }`; `has_origin(dir)` (`remote get-url origin`); a process-wide lock per canonical repo path (`static FETCHING: OnceLock<Mutex<HashMap<PathBuf, Arc<Mutex<Option<Instant>>>>>>`, the inner value = when it last fetched), taken with `try_lock` polling every 50 ms until the deadline (`Failed("another fetch of this repo is still running")` past it), skipping the fetch if the last one finished < 5 s ago; `fetch --quiet --no-tags origin` through `git_with_timeout`; on success, if `refs/remotes/origin/HEAD` is missing, `remote set-head origin --auto` with the deadline (failure ignored); `resolve_base(dir, opts)`: first of `origin/HEAD` (name via `symbolic-ref --short refs/remotes/origin/HEAD`), `origin/main`, `origin/master`, `main`, `master`, `HEAD` that passes `rev-parse --verify --quiet <ref>^{commit}`; `pub fn prepare_base(...)`. Register `mod base` and re-export `RepoBase`, `FetchOutcome`, `prepare_base` in `crates/delilah-git/src/lib.rs`.
- [X] T006 Make the existing `fetch(dir, opts)` in `crates/delilah-git/src/workspace.rs` take the same per-repo lock (blocking, no deadline) so archive-on-merge never races a start; keep its signature. Add a test in `crates/delilah-git/tests/base.rs` that `fetch` and `prepare_base` on one repo from two threads both succeed.

**Checkpoint**: `cargo test -p delilah-git` green; nothing calls `prepare_base` yet.

---

## Phase 3: User Story 1 - A new worktree task starts from the remote's latest default branch (Priority: P1) 🎯 MVP

**Goal**: repo-project worktrees start from the fresh remote default branch.

**Independent Test**: stale local `main`, start a worktree task → the branch contains the remote's new commit, has no upstream, the project folder is unchanged.

- [X] T007 [US1] Write failing tests in `crates/delilah-git/tests/workspace.rs`: with `fx.repo()` and `advance_remote(svc-orders)`, `create_workspace(&repo, &fx.repo_spec(), &opts(), Deadline::after(30s))` → the workspace HEAD == the remote's new head, `upstream_of(ws, branch)` is `None`, the repo's local `main` and checked-out branch are unchanged, and the returned bases are `[RepoBase { repo: ".", refname: "origin/main", fetch: Fetched, .. }]`; with the repo checked out on another local branch (`git checkout -b other HEAD~0`), the task branch still starts at the remote head; with `origin` set to `/nonexistent`, the start succeeds from the old `origin/main` and the base is `Failed(_)`; an existing workspace folder / invalid branch name still fail as before (FR-011).
- [X] T008 [US1] Implement T007 in `crates/delilah-git/src/workspace.rs`: `create_workspace(root, spec, opts, deadline) -> Result<(Workspace, Vec<RepoBase>), GitError>`; the existing checks first (folder, branch name, submodules), then for `ProjectKind::Repo`: `prepare_base(root, ".", …)` and `worktree add -b <branch> <path> <sha>`. Leave the `Parent` arm on `HEAD` for now but adapt it to the new return type (empty bases); update the doc comment.
- [X] T009 [US1] Adapt every existing caller of `create_workspace` to the new signature: the tests in `crates/delilah-git/tests/workspace.rs` (pass `Deadline::after(Duration::from_secs(30))`, take `.0`), the `started()` test helper in `crates/delilah-daemon/src/actions.rs` (~line 332) and any other test helper found with `grep -rn create_workspace crates`.
- [X] T010 [US1] In `crates/delilah-daemon/src/run.rs::start_run`: build `let deadline = Deadline::after(Duration::from_secs(30));` before `spawn_blocking`, pass it to `create_workspace`, carry the bases out of the closure (`(ws, previous, bases)`), and after a successful start log each base with `tracing::info!(task = %key, repo = %b.repo, base = %b.refname, sha = %b.sha, fetch = ?b.fetch, "task branch base")` (FR-010). Extract the base-handling into a small `fn report_bases(app, key, &bases)` (async) so US4 can add the notices there.
- [X] T011 [US1] Write a daemon test in `crates/delilah-daemon/src/run.rs` (tests module, next to the existing `run(key)` helper ~line 679) that starts a worktree task on a fixture repo project whose remote was advanced and checks the workspace's HEAD is the remote head.

**Checkpoint**: US1 works for repo projects in worktree mode; `cargo test -p delilah-git -p delilah-daemon` green.

---

## Phase 4: User Story 2 - Checkout-mode tasks start from the fresh default branch too (Priority: P1)

**Goal**: new branches in the project's own folder start from the fresh remote default; existing ones are untouched.

**Independent Test**: stale local `main`, clean folder, start a checkout task → the folder is on a new branch at the remote head; with the branch already existing → switched to it, same sha.

- [X] T012 [US2] Write failing tests in `crates/delilah-git/tests/workspace.rs`: `use_checkout(&repo, &spec, &opts(), deadline)` after `advance_remote` → the folder is on the task branch at the remote head, `upstream_of` is `None`, local `main` unchanged, `previous == [(".", "main")]`, base `Fetched`; with the task branch already existing at sha X (create it, switch back to `main`) → switched to it, still at X, base `fetch == Existing` and no fetch needed (works with `origin` set to `/nonexistent`); a dirty folder still fails with `GitError::Dirty` before any fetch (point `origin` at a hanging `core.sshCommand` and assert it returns immediately); `leave_checkout` still goes back to `main`.
- [X] T013 [US2] Implement T012 in `crates/delilah-git/src/workspace.rs::use_checkout(root, spec, opts, deadline) -> Result<(Workspace, Vec<(String, String)>, Vec<RepoBase>), GitError>`: keep the dirty check first; per folder, if the branch exists → `checkout -q <branch>` and an `Existing` base (refname = the branch, sha = its tip); else `prepare_base(dir, location, …)` and `checkout -q -b <branch> <sha>`; keep the rollback of already-switched folders on error.
- [X] T014 [US2] Adapt the callers of `use_checkout`: `crates/delilah-daemon/src/run.rs::start_run` (pass the deadline, carry the bases to `report_bases`) and the existing tests in `crates/delilah-git/tests/workspace.rs` and `crates/delilah-daemon/src/actions.rs`.

**Checkpoint**: US1 + US2 green.

---

## Phase 5: User Story 3 - Parent projects: each chosen submodule starts from its own fresh default branch (Priority: P2)

**Goal**: parent worktree on the parent's fresh default; each chosen submodule's branch on its own fresh default, in both modes.

**Independent Test**: advance `svc-payments`'s remote (not recorded by the parent), start a task with it chosen → its branch contains the commit.

- [X] T015 [US3] Write failing tests in `crates/delilah-git/tests/workspace.rs` with `fx.parent()`: (a) worktree: after `advance_remote(remotes/svc-payments.git)` and `advance_remote(remotes/parent.git)`, `create_workspace(&fx.parent(), &fx.spec(&["svc-payments"]), …)` → the parent worktree HEAD (detached) == the parent remote's new head, `svc-payments` HEAD == its remote's new head on the task branch with no upstream, unchosen submodules not initialised, bases for `.` and `svc-payments` both `Fetched`; (b) checkout mode on `fx.parent()` with `svc-payments` and `svc-billing` chosen, after `git -C <parent>/svc-billing remote set-url origin /nonexistent`: `svc-billing` is `Failed` and starts from its last fetched `origin/main`, `svc-payments` is `Fetched` and fresh (FR-004a); (c) checkout mode on `fx.parent()` with `svc-payments` chosen → the submodule is on the task branch at its remote's new head, the parent folder's HEAD is unchanged and no base is reported for `.`; (d) checkout mode with the branch already existing in one chosen submodule and not in the other → the first is `Existing` and unmoved, the second is fresh.
- [X] T016 [US3] Implement T015 in `crates/delilah-git/src/workspace.rs`: `Parent` arm of `create_workspace` → `prepare_base(root, ".", …)`, `worktree add --detach <path> <sha>`, `submodule update --init -- <chosen>` (unchanged), then `init_submodules` per submodule: `prepare_base(ws/sub, sub, …)` and `checkout -q -b <branch> <sha>`, collecting the bases (rollback of the worktree on error as today). `use_checkout` already handles submodules per folder from T013; make sure `checkout_dirs` never includes the parent folder for a parent project.

**Checkpoint**: US1–US3 green.

---

## Phase 6: User Story 4 - Offline: the task still starts, and the user is told (Priority: P2)

**Goal**: a failed fetch becomes a dismissable Bandeja item and a banner on the task.

**Independent Test**: `origin` unreachable, start a task → it starts; `GET /api/v1/inbox` has a `base-stale` item with `notice`; dismiss removes it.

### Daemon

- [X] T017 [US4] Write failing tests in `crates/delilah-daemon/src/run.rs` (tests module): starting a task on a fixture repo project whose `origin` is `/nonexistent` succeeds, and the store has one open notice `kind == "base-stale"`, `repo == "."`, `cause == "start"`, text naming the repo, the reason and `origin/main @ <short sha>`; a successful fetch records none; a task whose base is `NoRemote` or `Existing` records none.
- [X] T018 [US4] Implement T017 in `crates/delilah-daemon/src/run.rs::report_bases`: for each `FetchOutcome::Failed(reason)` insert a notice through the store's existing notice insert (`insert_notice_in` in `crates/delilah-daemon/src/store_prs.rs`; add a public `insert_notice(key, &NewNotice)` wrapper if none is exposed) with `kind = KIND_BASE_STALE` (`pub(crate) const KIND_BASE_STALE: &str = "base-stale";` in `run.rs`), `cause = "start"`, text via `app.lang().pick(es, en)` — es: "No se pudo actualizar {repo} ({reason}); la rama empieza en {refname} @ {sha7} y puede ir atrasada.", en: "Could not update {repo} ({reason}); the branch starts at {refname} @ {sha7} and may be behind." (`.` shown as the project's name) — and `detail` JSON `{refname, sha, reason}`; then `app.emit(Topic::Inbox)`. Errors are logged with `tracing::warn!`, never returned.
- [X] T019 [US4] Write failing tests in `crates/delilah-daemon/src/inbox.rs` (tests module, next to `open_pr_notices_are_items_after_their_neighbours_and_resolved_ones_are_not` ~line 690): an open `base-stale` notice of an active task with no PR is listed as `kind: "base-stale"` with `notice: Some(id)`, `pr: None`, the notice's text; once resolved (`POST /api/v1/pr-notices/{id}/dismiss`) it is gone; archiving the task resolves it; a terminal task without workspace does not list it; the JSON of a non-notice item has no `notice` key.
- [X] T020 [US4] Implement T019 in `crates/delilah-daemon/src/inbox.rs`: `ItemKind::BaseStale` (serialises `"base-stale"`), `InboxItem.notice: Option<i64>` with `#[serde(skip_serializing_if = "Option::is_none")]` (set `None` in every existing literal), and in the notices loop handle `KIND_BASE_STALE` before the PR lookup.

### UI

- [X] T021 [P] [US4] Write failing tests in `app/src/components/InboxView.test.tsx`: a `base-stale` item shows the label "Rama posiblemente atrasada" and its text, and "Descartar" calls `api.dismissNotice(item.notice)`; it offers no PR actions.
- [X] T022 [P] [US4] Write failing tests in `app/src/components/TaskScreen.test.tsx`: with `notices` containing a `base-stale` item, a banner shows its text; "Descartar" calls `api.dismissNotice` and `onChanged`; with no notices there is no banner.
- [X] T023 [US4] Implement the contract in `app/src/api.ts`: `InboxKind` += `"base-stale"`, `InboxItem.notice?: number`; not added to `BLOCKING`. Add `"base-stale": "rama posiblemente atrasada"` to `SAYS` in `app/src/alerts.ts`.
- [X] T024 [US4] Implement T021 in `app/src/components/InboxView.tsx`: the `KIND` entry (`label: "Rama posiblemente atrasada"`, the existing warning icon/class used elsewhere in the file, else `icon: "failed", cls: "c-warn"` or the closest existing class) and a "Descartar" button for `base-stale` items with `item.notice`, calling `api.dismissNotice`.
- [X] T025 [US4] Implement T022 in `app/src/components/TaskScreen.tsx` (new prop `notices: InboxItem[]`, a banner above the tabs) and pass the selected task's `base-stale` items from `app/src/App.tsx` (filter the inbox by `task` and `kind`).
- [X] T026 [P] [US4] Add the English strings for every new `t("…")` in `app/src/i18n/en.ts` ("Rama posiblemente atrasada" → "Branch may be behind", "Descartar" if not already there).

**Checkpoint**: all stories green; `cd app && npx tsc --noEmit && npx vitest run` green.

---

## Phase 7: Polish & Cross-Cutting Concerns

- [X] T027 [P] Update `docs/architecture.md` (Workspaces: fetch + base resolution + sha start point + deadline + lock; `delilah-git` row mentions `base.rs`; `pr_notices` kinds include `base-stale`) and `docs/status.md` (built: fresh task bases; known limit: parent worktrees still need the network to init submodules).
- [X] T028 [P] Add a decision to `docs/decisions.md`: task branches start from `origin`'s default branch as just fetched, by sha (no upstream); a failed fetch warns and continues; local `main` is never fast-forwarded; per-project base out of scope; why `pr_notices` was reused.
- [X] T029 [P] Update `README.md`: where a new task's branch starts, and the "Rama posiblemente atrasada" warning.
- [ ] T030 Run the whole suite: `cargo test --workspace`, `cargo test --manifest-path app/src-tauri/Cargo.toml`, `cd app && npx tsc --noEmit && npx vitest run`; fix what fails.
- [ ] T031 Walk `specs/005-fresh-base-branch/quickstart.md` in a preview (`scripts/preview.sh --fresh`, never 7420), scenarios 1–5.
- [ ] T032 Bump the version 0.4.0 → 0.5.0 (minor: new backwards-compatible behaviour, additive API) in the four places listed in `docs/development.md#releasing` plus the lockfiles (`Cargo.lock`, `app/src-tauri/Cargo.lock`, `app/package-lock.json` as applicable), as its own commit `chore: bump version to 0.5.0`; re-check `main`'s version first.

---

## Dependencies & Execution Order

- **Setup (T001)** → **Foundational (T002–T006)** → stories.
- **US1 (T007–T011)** changes `create_workspace`'s signature and adds `report_bases`: do it before US2–US4.
- **US2 (T012–T014)** changes `use_checkout`'s signature; independent of US3 in tests, but both edit `workspace.rs`: run them in sequence.
- **US3 (T015–T016)** after US2 (relies on `use_checkout` per folder).
- **US4**: daemon (T017–T020) after US1 (`report_bases`); UI (T021–T026) only needs the contract and can run alongside the daemon part.
- **Polish** last; T032 is the last commit before the PR.

### Parallel opportunities

- T021, T022, T026 (different UI files) together; T023–T025 after them.
- T027, T028, T029 (different docs) together.
- Daemon US4 (T017–T020, Rust) and UI US4 (T021–T026, TS) share no files.

### Parallel example: US4

```text
Package A (Rust): T017 → T018 → T019 → T020   (run.rs, store_prs.rs, inbox.rs)
Package B (TS):   T021 + T022 + T026 → T023 → T024 → T025   (app/src/...)
```

## Implementation Strategy

- **MVP**: Setup + Foundational + US1 — worktree tasks on repo projects start fresh, which is the reported pain.
- Then US2 (checkout mode is the default, so most tasks), US3 (parents), US4 (visible warning; until then a failed fetch is only in the log).
- Each phase ends green and committed; contract review when US4 touches `inbox.rs` ↔ `api.ts`.
