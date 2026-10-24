# Progress — 005-fresh-base-branch

Branch: feature/LIBRE-12 · Base: 4c8b4cd · Started: 2026-10-24
Mode: format=markdown commits=yes autonomy=auto
Plan: max parallel — WP-07 alongside WP-01..; WP-05 ∥ WP-06 after WP-04; phases 5+6 reviewed together.

| WP | Phase | Group | Status | Attempts | Commit | Notes |
|----|-------|-------|--------|----------|--------|-------|
| WP-01 | 1 | 1A | done | 1 | e328fcd | |
| WP-02 | 2 | 2A | done | 1 | 63c6b96 | |
| WP-03 | 3 | 3A | done | 1 | c1b38e3 | |
| WP-04 | 4 | 4A | done | 1 | a9df67f | |
| WP-05 | 5 | 5A | done | 1 | 24cbf7e | parallel with WP-06 |
| WP-06 | 6 | 6A | done | 1 | 93a44b6 | parallel with WP-05 |
| WP-07 | 6 | 6A | done | 1 | 29ae922 | started early, UI only |
| WP-08 | 7 | 7A | verified | 1 | 133c65c | |
| WP-09 | 7 | 7B | done | 1 | | rebased on main, full suite green |
| WP-10 | 7 | 7C | done | 1 | 99d1498 | 0.8.0 → 0.9.0 (main had moved past 0.5.0) |

## Findings (closed after the branch review)

| Phase | Reviewer | Severity | Finding | File | Resolution |
|-------|----------|----------|---------|------|------------|
| 2 | spec+architecture | MEDIUM | Fetch lock keyed on folder path, not `git rev-parse --git-common-dir`: worktree archive-on-merge and a start on the project root use different locks (T006 goal unmet in worktree mode) | crates/delilah-git/src/base.rs (repo_lock) | fixed 2d1946c: keyed on the common git dir |
| 2 | architecture+security | MEDIUM | git_with_timeout joins reader threads unbounded: a grandchild holding the pipe (ssh ControlPersist, credential helper) breaks the deadline; kill shells out to `kill` via PATH, errors ignored | crates/delilah-git/src/command.rs (git_with_timeout) | fixed 9c922b8: channels, bounded waits; kill shells out to `kill` by design (decision 44) |
| 2 | architecture | MEDIUM | `fetch()` holds the shared lock around an unbounded `git()` (no deadline, no GIT_TERMINAL_PROMPT=0, no `--no-tags`, doesn't record last-fetch time): a hung fetch blocks every later start | crates/delilah-git/src/workspace.rs (fetch) | fixed 2d1946c: goes through locked_fetch with a deadline |
| 2 | security | MEDIUM | Only GIT_TERMINAL_PROMPT=0: ssh/askpass/credential-helper prompts (GUI/keychain) can still appear; consider BatchMode=yes, GIT_ASKPASS/SSH_ASKPASS, credential.interactive=never | crates/delilah-git/src/command.rs | fixed 69725ba: GIT_ASKPASS, SSH_ASKPASS_REQUIRE, credential.interactive |
| 2 | architecture+spec | MEDIUM | No usable base encoded as `refname "HEAD", sha ""` (stringly typed); callers must check | crates/delilah-git/src/base.rs (prepare_base) | fixed 69725ba: `sha: Option<String>`, `RepoBase::start` |
| 2 | security | LOW | Fetch runs commands from repo config (core.sshCommand, credential.helper, gitProxy, insteadOf); decide whether agent-written config is trusted and record it | crates/delilah-git/src/base.rs | recorded in decision 66 (trusted, nothing may ask) |
| 2 | architecture | LOW | `FetchOutcome::Existing` is a caller concept in the git crate; failed fetches aren't cached so queued callers retry serially; blocking contract of prepare_base undocumented | crates/delilah-git/src/base.rs | kept, documented; failures now reused for 5 s (69725ba) |
| 2 | architecture | LOW | git_with_timeout duplicates git()'s setup/error mapping; GitError::Timeout is a new public variant | crates/delilah-git/src/command.rs | fixed 2d1946c: one `command()` and `checked()`; Timeout variant kept |
| 2 | spec | LOW | Untested: no child left after timeout, lock-wait past deadline → Failed, 5 s reuse window; test (f) indistinguishable from (a) | crates/delilah-git/tests/base.rs | fixed 69725ba: tests for each |
| 2 | security | LOW | repo_lock map never pruned (negligible) | crates/delilah-git/src/base.rs | kept: one small entry per repo (documented) |
| 3 | spec | LOW | No repo-project test (`fx.repo_spec()`) that an existing folder / invalid branch still fail before the new fetch (code order is right) | crates/delilah-git/tests/workspace.rs | fixed 69725ba: a_repo_workspace_refuses_before_any_fetch |
| 3 | spec | LOW | report_bases log line untested; T011 checks project folder only by HEAD != remote head | crates/delilah-daemon/src/run.rs | fixed cc8db3f (log) and this commit (folder) |
| 4 | spec | LOW | "Existing branch, no fetch" test uses `/nonexistent` (fails instantly) so it can't catch a regression that fetches; a hanging sshCommand would | crates/delilah-git/tests/workspace.rs | fixed 69725ba: hanging sshCommand |
| 4 | spec | LOW | Existing arm: `rev-parse HEAD` `.unwrap_or_default()` can log an empty sha silently | crates/delilah-git/src/workspace.rs (use_checkout) | fixed 9c922b8 |
| 5 | spec | MEDIUM | US3 scenario 3 (submodules with different default branch names) has no parent-level test; correct by construction, `master` unit-tested in tests/base.rs | crates/delilah-git/tests/workspace.rs | fixed 9c922b8 |
| 5 | spec | LOW | Parent worktree: no test of a chosen submodule whose fetch fails or has no origin (empty-sha fallback path) | crates/delilah-git/tests/workspace.rs | fixed 9c922b8 (checkout mode; a worktree always clones from the remote) |
| 6 | contract | MEDIUM | base-stale notice for repo "." is silently resolved by `link_pr` (new URL) / `unlink_pr`: their UPDATE has no `kind` filter | crates/delilah-daemon/src/store_prs.rs:183,269 | fixed 9c922b8 |
| 6 | contract | MEDIUM | A dismissed warning never comes back for the same task+repo: UNIQUE(task_key, repo, kind, cause) + INSERT OR IGNORE with constant cause "start" | crates/delilah-daemon/src/run.rs:57 | fixed 9c922b8: cause is the branch |
| 6 | contract | LOW | base-stale has a SAYS entry so it raises a desktop alert though not in BLOCKING — confirm wanted | app/src/alerts.ts:23 | intended, decision 66 |
| 6 | contract | LOW | 0011 migration comment doesn't list base-stale (shipped, don't edit) | crates/delilah-daemon/migrations/0011_pull_requests.sql:41 | won't fix: shipped migration |
| 6 | spec | LOW | Missing English string for SAYS "rama posiblemente atrasada" (macOS notification via t()) | app/src/i18n/en.ts | fixed 69725ba |
| 6 | spec | LOW | Untested: archive → resolve via the real archive path, App.tsx notices filter, parent start with one failing submodule naming only it | crates/delilah-daemon/src/inbox.rs, app/src/App.tsx | fixed 9c922b8, 69725ba |
