## WP-20a — done (one environmental check not runnable)
Tasks: 1 ✓ (LinkError import already absent from capture.rs; clippy has no warnings), 2 ✓ (verified in git diff that SessionChain.tsx/TaskScreen.tsx filter replaced_by == null; removed the line from docs/status.md), 3 see below
Files: docs/status.md (line removed); `cargo fmt --all` reformatted some Rust files (pr_api/dto.rs `use axum::Json` ordering and possibly others)
Tests:
- cargo fmt --all --check: failed, ran cargo fmt --all, now clean
- cargo clippy --workspace --all-targets: no warnings/errors
- cargo test --workspace: delilah-daemon lib 1164 passed, 2 failed under load (hooks::tests::a_held_terminal_stop_applies_once_its_sub_agents_end_with_no_new_turn, pr_loop::tests::nudges_in_the_window_make_one_delayed_check); both pass when re-run alone (flaky, timing). The workspace run stopped at that crate, so later crates were not run in that invocation.
- cargo test --manifest-path app/src-tauri/Cargo.toml: cannot build: missing sidecar `binaries/delilahd-aarch64-apple-darwin` (environmental, not feature-related; also no app/dist, a stub dist was tried and removed)
- app: npx tsc --noEmit ok; npx vitest run --fsModuleCache=false: 115 files, 1368 tests passed. node_modules symlink removed.
- cargo test --workspace --no-fail-fast (re-run): all suites pass (daemon lib 1166/1166), so the two failures were load flakes.
