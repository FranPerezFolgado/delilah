# Progress — 009-chat-mode-permissions

Branch: 009-chat-mode-permissions · Base: 214330b · Started: 2026-10-26
Mode: format=markdown commits=yes autonomy=auto

| WP | Phase | Group | Status | Attempts | Commit | Notes |
|----|-------|-------|--------|----------|--------|-------|
| WP-01 | 1 | 1A | done | 1 | 32944f0 | R1 confirmed; no escalating control action found |
| WP-02 | 2 | 2A | done | 1 | a345669 | red weak: agents/fake tests written with code; 2 daemon flakes pass alone |
| WP-03 | 3 | 3A | done | 1 | 6954d08 | T015 code-first (no red); e2e needs cargo build -p delilah-daemon first |
| WP-04 | 4 | 4A | done | 1 | 88dfd94 | PERM-9, PERM-10 |
| WP-05 | 4 | 4A | done | 1 | dbab98f | tests written with code (no red seen) |
| WP-03F | 3 | 3F | done | 1 | | FR-008 for project sessions, sleep → poll |
| WP-06 | 5 | 5A | done | 1 | | full suite green |
| WP-07 | 6 | 6A | done | 1 | 7ac18fa | decision #80 |
| WP-07F | 6 | 6F | done | 1 | | docs: permission_denied ignored, project-session hint |
| WP-08 | 6 | 6B | implementing | 0 | | |

## Open findings

| Phase | Reviewer | Severity | Finding | File |
|-------|----------|----------|---------|------|
| 5 | evidence | MEDIUM | daemon ignores `system/permission_denied` (no card, no pause, nothing shown); escalation channel under auto is unverified (spike found no escalating action) | crates/delilah-daemon |
| 4 | spec-compliance | LOW | `deny` under "default" has no e2e | crates/delilah-e2e/tests/chat_permissions.rs |
| 4 | spec-compliance | LOW | "null shows Auto" vitest passes even if the preference never loads (initial state is auto) | app/src/components/SettingsView.test.tsx |
| 3 | test | LOW | "no restart" check is a bounded 3 s sleep (justified in a comment) | crates/delilah-e2e/tests/chat_permissions.rs |
| 3 | test | LOW | add multi-line PTY screen test for rejection extraction | crates/delilah-daemon/src/run.rs |
| 3 | spec-compliance | MEDIUM | FR-008 hint skipped for project sessions (fail_if_mode_rejected returns early when project_of(task)) | crates/delilah-daemon/src/run.rs |
| 3 | spec-compliance | LOW | T015 written code-first, no red run | crates/delilah-daemon/src/run.rs |
| 3 | spec-compliance | LOW | T014(c) checks the no-pause-after-routine-turn condition only indirectly | crates/delilah-e2e/tests/chat_permissions.rs |
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

### WP-06 Evidence check
T019: the fake's `auto` semantics (T009: `allow` runs without asking, `ask` raises `can_use_tool`) and the e2e expectations (T014, T016) agree with the spike table (T002): with `--permission-mode auto` routine actions run with no `control_request`, without the flag they all arrive as `can_use_tool`. The spike found NO action that auto escalates, so the escalation channel (`can_use_tool` vs `system/permission_denied`) is **unverified**. Kept `auto: "ask"` as `can_use_tool`. `grep permission_denied crates/delilah-daemon`: no match, so the daemon ignores that event (no card, no pause); nothing broken, no daemon change. Noted in the spike doc.

### WP-06 Full suite
- `cargo build --workspace` first (e2e runs the built `delilahd`).
- `cargo test --workspace`: all green (agents 91, daemon 818, e2e 79 lib + `chat_permissions` 6 and the other e2e suites 9/4/22/8/59/19, 28 + 60 in smaller crates). The two WP-02 flakes (`a_message_sent_while_a_card_is_pending_is_queued`, `nudges_in_the_window_make_one_delayed_check`) passed this time.
- `cargo test --manifest-path app/src-tauri/Cargo.toml`: 4 passed. It first failed to build because the gitignored sidecar `app/src-tauri/binaries/delilahd-aarch64-apple-darwin` was missing; I copied `target/debug/delilahd` there (and made an empty `app/dist`), both gitignored.
- `cd app && npx tsc --noEmit`: clean. `npx vitest run`: 83 files, 857 tests passed (permission-card, remembered-rule and autopilot-pause tests included, SC-004).
- `cargo clippy --workspace --all-targets -- -D warnings` and `cargo fmt --all --check`: clean.
