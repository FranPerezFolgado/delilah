# Implementation Plan: App auto-update

**Branch**: `feature/LIBRE-10` | **Date**: 2026-10-23 | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `specs/005-app-auto-update/spec.md`

## Summary

Installed Delilah apps update themselves from GitHub Releases, with the user's consent, through Tauri 2's official updater plugin:

- The shell registers the updater and process plugins (in release builds only) and adds a "Buscar actualizaciones…" menu item.
- The web UI drives the flow: it checks at start-up, every 6 h and on demand, and shows a notice with the version and notes. Before installing it warns about live sessions and offers "Actualizar ahora", "Cuando terminen" or "Cancelar". Then it downloads with progress, verifies, installs and relaunches.
- After the relaunch, the existing code restarts the daemon, rewrites the plugin and runs the migrations. The daemon adds a pre-upgrade database copy.
- A new `scripts/release.sh` replaces the manual build:
  1. It checks versions, tag, key, `gh` account and repo visibility.
  2. It writes notes from conventional commits and builds the signed bundle.
  3. It generates `latest.json` and publishes a draft release, then makes it public last.

## Technical Context

**Language/Version**: Rust 2024 (daemon, Tauri shell); TypeScript + React (`app/`); Bash + Node ESM for the release script

**Primary Dependencies**: new `tauri-plugin-updater` 2.x, `tauri-plugin-process` 2.x, `@tauri-apps/plugin-updater`, `@tauri-apps/plugin-process` (pinned like the other plugins). Existing: tauri 2, sqlx, React, vitest; `gh` CLI for publishing; Node's built-in `node:test`.

**Storage**: no schema change. New files `$DELILAH_HOME/version` and `$DELILAH_HOME/backups/delilah-<prev>.db` ([data-model.md](data-model.md)).

**Testing**:
- `cargo test -p delilah-daemon` (`upgrade.rs`: marker, copy, no overwrite, pruning);
- vitest (`updates.ts` reducer; `UpdateNotice` and the Settings section with a fake `platform`);
- `node --test scripts/release/` (versions, notes, manifest).

The Tauri glue is checked with `cargo check --manifest-path app/src-tauri/Cargo.toml`, and the end-to-end path by hand ([quickstart.md](quickstart.md)).

**Target Platform**: macOS 13+, Apple Silicon (`darwin-aarch64`).

**Project Type**: desktop app (Tauri shell + React UI) with a local daemon.

**Performance Goals**: the check adds nothing to start-up (it is deferred 10 s). The update takes under 2 min end to end (SC-001).

**Constraints**:
- No updater in dev builds or previews.
- Never touch the installed daemon on 7420 while developing.
- Signing key and password stay out of the repo.
- Remote publishing only as `FranPerezFolgado`.

**Scale/Scope**: one maintainer and two Macs. There is one platform in the manifest.

## Constitution Check

`.specify/memory/constitution.md` is still the unfilled template, so the ground rules in `CLAUDE.md` are the gates:

| Gate | Status |
|---|---|
| Nothing from the work repos; fixtures only | ✅ tests use temp dirs and `Store::in_memory()`; the release script only touches this repo |
| Agent hooks/settings outside the repo, clean env | ✅ untouched |
| Try changes only in a preview, never 7420 / pkill | ✅ the quickstart uses `scripts/preview.sh`; the real update goes through the app, never by killing the daemon |
| Confirm the account before remote operations | ✅ `release.sh` refuses unless `gh` is on `FranPerezFolgado`; implementation does no remote operations (publishing a release is the maintainer's step) |
| Migrations never edit a shipped one | ✅ no migration |
| Contracts (`api.rs` ↔ `api.ts`) | ✅ unchanged; new contracts are the manifest, shell events and release CLI ([contracts/update-contracts.md](contracts/update-contracts.md)) |
| Every Spec Kit task bumps the version | ✅ **minor** (new user-facing feature, backwards compatible): 0.4.0 → 0.5.0, re-checked against `main` before the PR |

Post-design re-check: no change. One cost has to be accepted: `npm run dist` without the signing key now fails, because `createUpdaterArtifacts` needs the key. The docs replace that step with `scripts/release.sh`.

## Project Structure

### Documentation (this feature)

```text
specs/005-app-auto-update/
├── spec.md
├── plan.md              # this file
├── research.md          # R1–R14
├── data-model.md        # manifest, UI state machine, marker/backups
├── quickstart.md
├── contracts/update-contracts.md
├── checklists/          # requirements.md, release-safety.md
└── tasks.md             # /speckit-tasks
```

### Source Code (repository root)

```text
app/src-tauri/
├── Cargo.toml                  # + tauri-plugin-updater, tauri-plugin-process
├── tauri.conf.json             # bundle.createUpdaterArtifacts, plugins.updater {pubkey, endpoints}
├── capabilities/default.json   # + updater:default, process:allow-restart
└── src/main.rs                 # plugins (updater only in release), menu item → emit "check-updates"

app/
├── package.json                # + @tauri-apps/plugin-updater, @tauri-apps/plugin-process
└── src/
    ├── platform.ts             # canUpdate, appVersion, checkForUpdate, relaunch, onCheckUpdatesMenu
    ├── updates.ts              # pure reducer + types (data-model.md)
    ├── updates.test.ts
    ├── useUpdates.ts           # hook: timers, menu event, sessions count, drives platform calls
    ├── components/UpdateNotice.tsx (+ .test.tsx)   # banner in the main window
    ├── components/SettingsView.tsx                 # "Actualizaciones" section (+ test)
    ├── App.tsx                 # mounts UpdateNotice, wires useUpdates
    ├── i18n.ts / i18n/en.ts    # strings (es/en)
    └── styles.css

crates/delilah-daemon/src/
├── upgrade.rs                  # new: marker, VACUUM INTO copy, prune to 3
└── main.rs                     # call upgrade::before_open / after_open around Store::open

scripts/
├── release.sh                  # new: checks → notes → build → manifest → draft → upload → publish
└── release/
    ├── versions.mjs (+ .test.mjs)
    ├── notes.mjs    (+ .test.mjs)
    └── manifest.mjs (+ .test.mjs)

docs/  development.md (Releasing rewritten, key setup/backup, bootstrap), architecture.md (updates, data files),
       decisions.md (updater decisions), status.md; README.md (Actualizaciones)
```

**Structure Decision**: the code lands in the existing desktop app and daemon. The release tooling goes in `scripts/` next to `preview.sh`, with its pure logic in `.mjs` so it can be tested without new dependencies.

## Design notes

1. **UI flow ownership.** `useUpdates` is the only place with side effects: timers, platform calls and session polling. `updates.ts` is a pure `(state, meta, event) → (state, meta)` reducer. `UpdateNotice` renders the state and dispatches. The tests fake `platform` and `api.sessions`.
2. **Session count.** It comes from `api.sessions()` and counts `live && !closed`. In `waiting` it is refreshed on the app's existing live updates, with a 10 s poll as fallback. At 0 the download starts by itself.
3. **Menu.** The `CHECK_UPDATES` menu id emits `check-updates` to the main window. In debug builds the item is still shown but the UI answers it with "only in installed builds".
4. **Daemon copy.** `upgrade::before_open(home, db_path, current)` runs before `Store::open`, and `upgrade::after_open(home, current)` runs after it succeeds. A failed copy is logged, never fatal. A copy that already exists is never overwritten.
5. **Release safety.** Uploads go to a draft. `latest.json` is uploaded last, and the release is made public last. A rerun reuses the draft and uses `--clobber`. A published `vX.Y.Z` refuses a rerun.
6. **Bootstrap.** 0.5.0 is the first version with the updater. Each Mac installs it by hand once, and later versions come through the app.

## Risks

- **Repo visibility**: if the repo is private, `latest.json` returns 404 and no app sees updates. `release.sh` checks this, and making the repo public is the maintainer's decision before the first release.
- **Quarantine on swap**: research R8 expects none. Quickstart step 3 checks it on the first real update.
- **Lost key**: it needs a manual reinstall on both Macs. The docs ask for a backup.

## Complexity Tracking

No gate violations.
