## WP-19 part 1 (T055) — done
Files: crates/delilah-core/src/hierarchy.rs, crates/delilah-core/src/project.rs — rustfmt only.
- cargo fmt --all: applied (only those 2 files changed)
- cargo clippy --workspace --all-targets -- -D warnings: clean, no fixes needed
- cargo test --workspace: all ok (1092, 149, 89, 85, 61, 45, 30, 19, 8, 7, 4 passed; 0 failed; flaky test did not fail)
- cargo test --manifest-path app/src-tauri/Cargo.toml: 6 passed. First run failed in build script: sidecar binaries/delilahd-aarch64-apple-darwin missing (gitignored, environmental); passed with a temporary copy of target/debug/delilahd, removed afterwards.
- app: npx tsc --noEmit clean; npx vitest run: 114 files, 1327 tests passed (node_modules symlink removed after).
