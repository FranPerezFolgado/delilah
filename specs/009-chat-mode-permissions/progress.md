# Progress — 009-chat-mode-permissions

Branch: 009-chat-mode-permissions · Base: 214330b · Started: 2026-10-26
Mode: format=markdown commits=yes autonomy=auto

| WP | Phase | Group | Status | Attempts | Commit | Notes |
|----|-------|-------|--------|----------|--------|-------|
| WP-01 | 1 | 1A | done | 1 | 32944f0 | R1 confirmed; no escalating control action found |
| WP-02 | 2 | 2A | done | 1 | a345669 | red weak: agents/fake tests written with code; 2 daemon flakes pass alone |
| WP-03 | 3 | 3A | verified | 1 | | T015 code-first (no red); e2e needs cargo build -p delilah-daemon first |
| WP-04 | 4 | 4A | pending | 0 | | |
| WP-05 | 4 | 4A | verified | 1 | | tests written with code (no red seen) |
| WP-06 | 5 | 5A | pending | 0 | | |
| WP-07 | 6 | 6A | verified | 1 | 7ac18fa | decision #80 |
| WP-08 | 6 | 6B | pending | 0 | | |

## Open findings

| Phase | Reviewer | Severity | Finding | File |
|-------|----------|----------|---------|------|
| 2 | spec-compliance | MEDIUM | no real red run for agents/fake tests (written with code) | specs/009-chat-mode-permissions/progress.md |
| 2 | spec-compliance | MEDIUM | 2 daemon tests failed in full run, pass alone (conversation::a_message_sent_while_a_card_is_pending_is_queued, pr_loop::nudges_in_the_window_make_one_delayed_check); recheck in WP-06. tsc was run by the orchestrator: OK | crates/delilah-daemon |
| 2 | spec-compliance | LOW | fake reads auto mode by scanning record.args instead of args.rs permission_mode | crates/delilah-e2e/src/fake/stream.rs |
| 2 | spec-compliance | LOW | no test of load_agent_permissions() with unset/other stored value | crates/delilah-daemon/src/state.rs |
| 2 | contract | LOW | AgentPermissions type inserted between Step's doc comment and `export type Step` | app/src/api.ts |
| 2 | architecture | MEDIUM | AtomicU8 0/1 magic numbers; add from_u8/as u8 or AtomicBool | crates/delilah-daemon/src/state.rs |
| 2 | architecture | MEDIUM | "auto"/"default" duplicated in validation and loader; add AgentPermissions::parse | crates/delilah-daemon/src/preferences.rs |
| 2 | architecture | LOW | store read error silently falls back to Auto (more permissive); log a warning | crates/delilah-daemon/src/state.rs |
| 2 | architecture | LOW | comment the AgentPermissions → Permission mapping in launch_flags | crates/delilah-daemon/src/run.rs |
| 2 | security | LOW | unreadable store → Auto (informational) | crates/delilah-daemon/src/state.rs |

## Test log

### WP-02 Red
Daemon tests were written first. The agents and fake-scenario tests were written alongside the code. No separate red run was made against reverted code. Without `Permission::Auto` / `AgentPermissions` the new tests fail to compile (compile errors count as failing).

### WP-02 Green
- `cargo test -p delilah-agents`: 91 passed.
- `cargo test -p delilah-e2e --lib`: 79 passed.
- `cargo test -p delilah-daemon`: 813 passed, 2 failed (`conversation::tests::a_message_sent_while_a_card_is_pending_is_queued`, `pr_loop::tests::nudges_in_the_window_make_one_delayed_check`). Both pass when re-run alone, so they are flaky under load.
- `cargo fmt --all` and `cargo clippy --workspace --all-targets -- -D warnings`: clean.
- `npx tsc --noEmit` in `app/`: not run, `node_modules` is not installed in this worktree.

### WP-03 Red
With the call site in `spawn_session` temporarily passing `AgentPermissions::Default` (edited back by hand afterwards; `cargo build -p delilah-daemon` first, since the e2e uses the built `delilahd`): `routine_work_in_chat_mode_raises_no_permission_card` failed (no `--permission-mode auto` in argv), `a_runtime_switch_relaunches_with_the_current_mode` failed, and so did `chat_mode_autopilot_runs_routine_work_and_pauses_only_on_a_real_ask`. The T015 code was written before its tests (only the unit tests and the two start-failure e2e tests cover it, all green); no red run for it.

### WP-03 Green
- `cargo test -p delilah-e2e`: 79 lib + 6 `chat_permissions` passed.
- `cargo test -p delilah-daemon`: 817 passed.
- `cargo clippy --workspace --all-targets -- -D warnings` and `cargo fmt --all --check`: clean.
