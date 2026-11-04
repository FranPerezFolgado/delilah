## WP-20b — done
Tasks:    T048 ✓
Files:    CHANGELOG.md, Cargo.toml, Cargo.lock, app/package.json, app/package-lock.json, app/src-tauri/Cargo.toml, app/src-tauri/Cargo.lock, app/src-tauri/tauri.conf.json (all modified by scripts/bump-version.sh patch)
Tests:    cargo check --workspace: pass (crates at v1.4.2)
Notes:    Version 1.4.1 (origin/main) -> 1.4.2. No --commit. git-cliff present; script did not refuse on dirty tree. git-cliff warned: tag v1.4.1 already exists on HEAD~ and 33 commits skipped for parse errors (normal). Eight files newly modified in git status (M).
