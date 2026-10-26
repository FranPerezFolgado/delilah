# Progress — 009-chat-mode-permissions

Branch: 009-chat-mode-permissions · Base: 214330b · Started: 2026-10-26
Mode: format=markdown commits=yes autonomy=auto

| WP | Phase | Group | Status | Attempts | Commit | Notes |
|----|-------|-------|--------|----------|--------|-------|
| WP-01 | 1 | 1A | done | 1 | 32944f0 | R1 confirmed; no escalating control action found |
| WP-02 | 2 | 2A | verified | 1 | | red weak: agents/fake tests written with code; 2 daemon flakes pass alone |
| WP-03 | 3 | 3A | pending | 0 | | |
| WP-04 | 4 | 4A | pending | 0 | | |
| WP-05 | 4 | 4A | pending | 0 | | |
| WP-06 | 5 | 5A | pending | 0 | | |
| WP-07 | 6 | 6A | pending | 0 | | |
| WP-08 | 6 | 6B | pending | 0 | | |

## Open findings

| Phase | Reviewer | Severity | Finding | File |
|-------|----------|----------|---------|------|

## Test log

### WP-02 Red
Daemon tests were written first. The agents and fake-scenario tests were written alongside the code. No separate red run was made against reverted code. Without `Permission::Auto` / `AgentPermissions` the new tests fail to compile (compile errors count as failing).

### WP-02 Green
- `cargo test -p delilah-agents`: 91 passed.
- `cargo test -p delilah-e2e --lib`: 79 passed.
- `cargo test -p delilah-daemon`: 813 passed, 2 failed (`conversation::tests::a_message_sent_while_a_card_is_pending_is_queued`, `pr_loop::tests::nudges_in_the_window_make_one_delayed_check`). Both pass when re-run alone, so they are flaky under load.
- `cargo fmt --all` and `cargo clippy --workspace --all-targets -- -D warnings`: clean.
- `npx tsc --noEmit` in `app/`: not run, `node_modules` is not installed in this worktree.
