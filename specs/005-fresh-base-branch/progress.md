# Progress — 005-fresh-base-branch

Branch: feature/LIBRE-12 · Base: 96e7d23 · Started: 2026-10-24
Mode: format=markdown commits=yes autonomy=auto
Plan: max parallel — WP-07 alongside WP-01..; WP-05 ∥ WP-06 after WP-04; phases 5+6 reviewed together.

| WP | Phase | Group | Status | Attempts | Commit | Notes |
|----|-------|-------|--------|----------|--------|-------|
| WP-01 | 1 | 1A | done | 1 | 654fa24 | |
| WP-02 | 2 | 2A | done | 1 | ec9e4ce | |
| WP-03 | 3 | 3A | implementing | 0 | | |
| WP-04 | 4 | 4A | pending | 0 | | |
| WP-05 | 5 | 5A | pending | 0 | | parallel with WP-06 |
| WP-06 | 6 | 6A | pending | 0 | | parallel with WP-05 |
| WP-07 | 6 | 6A | verified | 1 | 5845a13 | started early, UI only |
| WP-08 | 7 | 7A | pending | 0 | | |
| WP-09 | 7 | 7B | pending | 0 | | |
| WP-10 | 7 | 7C | pending | 0 | | |

## Open findings

| Phase | Reviewer | Severity | Finding | File |
|-------|----------|----------|---------|------|
| 2 | spec+architecture | MEDIUM | Fetch lock keyed on folder path, not `git rev-parse --git-common-dir`: worktree archive-on-merge and a start on the project root use different locks (T006 goal unmet in worktree mode) | crates/delilah-git/src/base.rs (repo_lock) |
| 2 | architecture+security | MEDIUM | git_with_timeout joins reader threads unbounded: a grandchild holding the pipe (ssh ControlPersist, credential helper) breaks the deadline; kill shells out to `kill` via PATH, errors ignored | crates/delilah-git/src/command.rs (git_with_timeout) |
| 2 | architecture | MEDIUM | `fetch()` holds the shared lock around an unbounded `git()` (no deadline, no GIT_TERMINAL_PROMPT=0, no `--no-tags`, doesn't record last-fetch time): a hung fetch blocks every later start | crates/delilah-git/src/workspace.rs (fetch) |
| 2 | security | MEDIUM | Only GIT_TERMINAL_PROMPT=0: ssh/askpass/credential-helper prompts (GUI/keychain) can still appear; consider BatchMode=yes, GIT_ASKPASS/SSH_ASKPASS, credential.interactive=never | crates/delilah-git/src/command.rs |
| 2 | architecture+spec | MEDIUM | No usable base encoded as `refname "HEAD", sha ""` (stringly typed); callers must check | crates/delilah-git/src/base.rs (prepare_base) |
| 2 | security | LOW | Fetch runs commands from repo config (core.sshCommand, credential.helper, gitProxy, insteadOf); decide whether agent-written config is trusted and record it | crates/delilah-git/src/base.rs |
| 2 | architecture | LOW | `FetchOutcome::Existing` is a caller concept in the git crate; failed fetches aren't cached so queued callers retry serially; blocking contract of prepare_base undocumented | crates/delilah-git/src/base.rs |
| 2 | architecture | LOW | git_with_timeout duplicates git()'s setup/error mapping; GitError::Timeout is a new public variant | crates/delilah-git/src/command.rs |
| 2 | spec | LOW | Untested: no child left after timeout, lock-wait past deadline → Failed, 5 s reuse window; test (f) indistinguishable from (a) | crates/delilah-git/tests/base.rs |
| 2 | security | LOW | repo_lock map never pruned (negligible) | crates/delilah-git/src/base.rs |
