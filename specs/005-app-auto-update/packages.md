# Work packages — App auto-update

Source: specs/005-app-auto-update/tasks.md · 41 pending agent tasks (T005 is maintainer-only and left out) · 13 packages

Check names: `rust:delilah-daemon` = `cargo test -p delilah-daemon`; `shell` = `cargo check` debug + `--release` on `app/src-tauri` (and `cargo test --manifest-path app/src-tauri/Cargo.toml`); `app` = `cd app && npx tsc --noEmit && npx vitest related --run <files>`; `release-tooling` = `node --test scripts/release/`.

## WP-01 Updater dependencies and config
- Phase: 1 Setup
- Tasks: T001, T002, T003, T004
- Files: app/src-tauri/Cargo.toml, app/src-tauri/Cargo.lock, app/package.json, app/package-lock.json, app/src-tauri/tauri.conf.json, app/src-tauri/capabilities/default.json
- Depends on: —
- Parallel group: 1A
- Acceptance: FR-010, FR-024 — updater and process plugins pinned (Rust and npm at matching versions), updater artifacts on, endpoint fixed, pubkey left as `REPLACE_WITH_UPDATER_PUBKEY`, capabilities allow updater and relaunch; shell `cargo check` and `tsc` pass.
- Checks: shell, app (tsc only)
- Reviewers: security

## WP-02 Update state machine and platform boundary
- Phase: 2 Foundational
- Tasks: T006, T007, T008
- Files: app/src/updates.test.ts, app/src/updates.ts, app/src/platform.ts
- Depends on: WP-01
- Parallel group: 2A
- Acceptance: FR-003, FR-004, FR-005, FR-008 — pure `reduce` covers every data-model transition; platform helpers are no-ops/null when `!canUpdate`.
- Checks: app (src/updates.ts, src/platform.ts)
- Reviewers: —

## WP-03 Daemon pre-upgrade DB copy
- Phase: 3 US1 Install a new version
- Tasks: T010, T011, T012
- Files: crates/delilah-daemon/src/upgrade.rs, crates/delilah-daemon/src/lib.rs (guess: register `mod upgrade;` where the other modules live), crates/delilah-daemon/src/main.rs
- Depends on: —
- Parallel group: 3A
- Acceptance: FR-013, FR-014 — first start on a new version leaves `backups/delilah-<prev>.db` (never overwritten, last 3 kept, failure never blocks start) and writes the `version` marker after open.
- Checks: rust:delilah-daemon
- Reviewers: architecture, security

## WP-04 Update notice UI and hook
- Phase: 3 US1 Install a new version
- Tasks: T009, T013, T014, T015, T016, T018
- Files: app/src/components/UpdateNotice.test.tsx, app/src/components/UpdateNotice.tsx, app/src/styles.css, app/src/useUpdates.ts, app/src/useUpdates.test.ts, app/src/App.tsx, app/src/i18n/en.ts
- Depends on: WP-02
- Parallel group: 3A
- Acceptance: FR-001, FR-006, FR-007, FR-011, FR-012, FR-023 — banner for available/downloading/installing/error, first check at 10 s then every 6 h, "Actualizar" downloads with progress then relaunches, nothing runs when `canUpdate` is false.
- Checks: app (src/updates.ts, src/useUpdates.ts, src/components/UpdateNotice.tsx)
- Reviewers: architecture

## WP-05 Register plugins in the shell
- Phase: 3 US1 Install a new version
- Tasks: T017
- Files: app/src-tauri/src/main.rs
- Depends on: WP-01
- Parallel group: 3A
- Acceptance: FR-005, FR-012 — process plugin always registered; updater plugin only under `cfg(not(debug_assertions))`; both profiles compile.
- Checks: shell
- Reviewers: security

## WP-06 Release tooling library and tests
- Phase: 4 US2 Publish a release
- Tasks: T019, T020, T021, T022, T024
- Files: scripts/release/versions.test.mjs, scripts/release/notes.test.mjs, scripts/release/manifest.test.mjs, scripts/release/check.test.mjs, scripts/release/versions.mjs, scripts/release/notes.mjs, scripts/release/manifest.mjs, scripts/release/check.mjs
- Depends on: WP-01
- Parallel group: 4A
- Acceptance: FR-017, FR-018, FR-019 — versions read/write only the four version fields, notes grouped from conventional commits or the tag message, manifest matches contracts §1, `check.mjs` reports mismatch, missing secrets, published tag and placeholder pubkey.
- Checks: release-tooling
- Reviewers: contract, security

## WP-07 Release workflow and dry-run script
- Phase: 4 US2 Publish a release
- Tasks: T025, T026
- Files: .github/workflows/release.yml, scripts/release.sh
- Depends on: WP-06
- Parallel group: 4B
- Acceptance: FR-016, FR-017, FR-019, FR-022 — tag push builds, signs and publishes as draft then promotes; local `--dry-run` does the checks and build without uploading; secrets never echoed.
- Checks: release-tooling (actionlint if installed)
- Reviewers: security, architecture

## WP-08 Release cut command and Makefile
- Phase: 4 US2 Publish a release
- Tasks: T023, T027, T028
- Files: scripts/release/cut.test.mjs, scripts/release/cut.mjs, Makefile
- Depends on: WP-06
- Parallel group: 4B
- Acceptance: FR-016, FR-017 — `make release` preflights, picks and bumps the version, tags, confirms and pushes atomically, rolls back on failure; tried only against a local bare remote.
- Checks: release-tooling
- Reviewers: security

## WP-09 Session warning and "Cuando terminen"
- Phase: 5 US3 Don't lose running work
- Tasks: T029, T030, T031
- Files: app/src/updates.test.ts, app/src/useUpdates.test.ts, app/src/useUpdates.ts, app/src/components/UpdateNotice.tsx, app/src/components/UpdateNotice.test.tsx, app/src/i18n/en.ts
- Depends on: WP-04
- Parallel group: 5A
- Acceptance: FR-009 — with live sessions "Actualizar" asks (now / when finished / cancel); waiting polls every 10 s and installs at 0; a failing `api.sessions()` never installs.
- Checks: app (src/updates.ts, src/useUpdates.ts, src/components/UpdateNotice.tsx)
- Reviewers: —

## WP-10 App menu item "Buscar actualizaciones…"
- Phase: 6 US4 Check on demand
- Tasks: T032
- Files: app/src-tauri/src/main.rs
- Depends on: WP-05
- Parallel group: 6A
- Acceptance: FR-002 — menu item emits `check-updates` to the main window, in debug and release builds.
- Checks: shell
- Reviewers: —

## WP-11 Settings "Actualizaciones" section and menu wiring
- Phase: 6 US4 Check on demand
- Tasks: T033, T034
- Files: app/src/components/SettingsView.test.tsx, app/src/components/SettingsView.tsx, app/src/App.tsx, app/src/useUpdates.ts, app/src/components/UpdateNotice.tsx, app/src/i18n/en.ts
- Depends on: WP-09
- Parallel group: 6A
- Acceptance: FR-002, FR-020, FR-023 — Settings shows installed version, last check and result, "Buscar ahora"; menu event triggers a manual check; checking/upToDate rendered; dev builds answer "solo en la app instalada".
- Checks: app (src/components/SettingsView.tsx, src/useUpdates.ts, src/components/UpdateNotice.tsx)
- Reviewers: —

## WP-12 Docs
- Phase: 7 Polish
- Tasks: T035, T036, T037, T038, T039
- Files: docs/development.md, docs/architecture.md, docs/decisions.md, docs/status.md, README.md, CLAUDE.md
- Depends on: WP-08, WP-11
- Parallel group: 7A
- Acceptance: FR-021 — release steps, key setup and backup, bootstrap, backups restore, architecture and decisions, status, README and CLAUDE.md versioning pointer are updated and true.
- Checks: —
- Reviewers: —

## WP-13 Version bump, full checks, PR text
- Phase: 7 Polish
- Tasks: T040, T041, T042
- Files: Cargo.toml, Cargo.lock, app/src-tauri/Cargo.toml, app/src-tauri/Cargo.lock, app/package.json, app/package-lock.json, app/src-tauri/tauri.conf.json
- Depends on: WP-12, WP-03
- Parallel group: 7B
- Acceptance: FR-013, FR-015 — minor bump (0.4.0 to 0.5.0 if main unchanged) in its own commit `chore: bump version to 0.5.0`; all full checks and the preview run pass; PR description lists level, maintainer steps and what a real tag must verify.
- Checks: rust:delilah-daemon (cargo test --workspace), shell, app (tsc + vitest run), release-tooling
- Reviewers: —
