# Implementation Plan: App auto-update

**Branch**: `feature/LIBRE-10` | **Date**: 2026-10-23 | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `specs/005-app-auto-update/spec.md`

## Summary

Installed Delilah apps update themselves from GitHub Releases, with the user's consent, through Tauri 2's official updater plugin:

- The shell registers the updater and process plugins (in release builds only) and adds a "Buscar actualizaciones…" menu item.
- The web UI drives the flow: it checks at start-up, every 6 h and on demand, and shows a notice with the version and notes. Before installing it warns about live sessions and offers "Actualizar ahora", "Cuando terminen" or "Cancelar". Then it downloads with progress, verifies, installs and relaunches.
- After the relaunch, the existing code restarts the daemon, rewrites the plugin and runs the migrations. The daemon adds a pre-upgrade database copy.
- Releasing becomes: bump, merge, then `git tag vX.Y.Z && git push origin vX.Y.Z`. A GitHub Actions workflow (`.github/workflows/release.yml`, macOS Apple Silicon runner) then:
  1. checks the tag against the four version files and that the release isn't already published;
  2. builds notes from the annotated tag message or the conventional commits;
  3. builds and signs with `tauri-action` (key from repository secrets), uploads the files and `latest.json` to a draft, and publishes it last.
- `scripts/release.sh X.Y.Z --dry-run` runs the same checks and the signed build locally, without publishing.

## Technical Context

**Language/Version**: Rust 2024 (daemon, Tauri shell); TypeScript + React (`app/`); GitHub Actions YAML, Bash + Node ESM for the release checks and dry run

**Primary Dependencies**: new `tauri-plugin-updater` 2.x, `tauri-plugin-process` 2.x, `@tauri-apps/plugin-updater`, `@tauri-apps/plugin-process` (pinned like the other plugins). Existing: tauri 2, sqlx, React, vitest; `tauri-apps/tauri-action@v0` in CI; `gh` CLI in the workflow and dry run; Node's built-in `node:test`.

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
- Publishing happens only in the repo's own workflow (its `GITHUB_TOKEN`); the signing key exists only as repository secrets and the maintainer's backup.

**Scale/Scope**: one maintainer and two Macs. There is one platform in the manifest.

## Constitution Check

`.specify/memory/constitution.md` is still the unfilled template, so the ground rules in `CLAUDE.md` are the gates:

| Gate | Status |
|---|---|
| Nothing from the work repos; fixtures only | ✅ tests use temp dirs and `Store::in_memory()`; the release script only touches this repo |
| Agent hooks/settings outside the repo, clean env | ✅ untouched |
| Try changes only in a preview, never 7420 / pkill | ✅ the quickstart uses `scripts/preview.sh`; the real update goes through the app, never by killing the daemon |
| Confirm the account before remote operations | ✅ implementation does no remote operations. Setting the secrets, making the repo public and pushing the tag are the maintainer's steps, done as `FranPerezFolgado` (quickstart) |
| Migrations never edit a shipped one | ✅ no migration |
| Contracts (`api.rs` ↔ `api.ts`) | ✅ unchanged; new contracts are the manifest, shell events and release CLI ([contracts/update-contracts.md](contracts/update-contracts.md)) |
| Every Spec Kit task bumps the version | ✅ **minor** (new user-facing feature, backwards compatible): 0.4.0 → 0.5.0, re-checked against `main` before the PR |

Post-design re-check: no change. One cost has to be accepted: `npm run dist` without the signing key now fails, because `createUpdaterArtifacts` needs the key. Releases run in the tag workflow, and local builds use `scripts/release.sh --dry-run`.

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

.github/workflows/
└── release.yml                 # new: on tag v*.*.* → check → notes → tauri-action (draft) → publish

scripts/
├── release.sh                  # new: --dry-run only (checks, signed build, local latest.json)
└── release/
    ├── check.mjs                # shared checks (CI + dry run)
    ├── versions.mjs (+ .test.mjs)
    ├── notes.mjs    (+ .test.mjs)
    └── manifest.mjs (+ .test.mjs)

docs/  development.md (Releasing rewritten, key setup/backup, bootstrap), architecture.md (updates, data files),
       decisions.md (updater decisions), status.md; README.md (Actualizaciones)
```

**Structure Decision**: the code lands in the existing desktop app and daemon. The release runs in a new GitHub workflow. Its checks and notes are pure `.mjs` in `scripts/release/`, shared with the local dry run and tested without new dependencies.

## Design notes

1. **UI flow ownership.** `useUpdates` is the only place with side effects: timers, platform calls and session polling. `updates.ts` is a pure `(state, meta, event) → (state, meta)` reducer. `UpdateNotice` renders the state and dispatches. The tests fake `platform` and `api.sessions`.
2. **Session count.** It comes from `api.sessions()` and counts `live && !closed`. In `waiting` it is refreshed on the app's existing live updates, with a 10 s poll as fallback. At 0 the download starts by itself.
3. **Menu.** The `CHECK_UPDATES` menu id emits `check-updates` to the main window. In debug builds the item is still shown but the UI answers it with "only in installed builds".
4. **Daemon copy.** `upgrade::before_open(home, db_path, current)` runs before `Store::open`, and `upgrade::after_open(home, current)` runs after it succeeds. A failed copy is logged, never fatal. A copy that already exists is never overwritten.
5. **Release safety.** `tauri-action` uploads to a draft, and the last workflow step publishes it only if everything before it passed. `releases/latest` never resolves to a draft. The workflow's `concurrency` is per tag. A rerun reuses the draft. A published `vX.Y.Z` fails the check.
6. **Bootstrap.** 0.5.0 is the first version with the updater. Its dmg comes from the release the workflow publishes. Each Mac installs it by hand once, and later versions come through the app.
7. **Workflow tests.** The workflow itself only runs on a real tag. Its logic lives in `check.mjs`/`notes.mjs` (unit-tested), and the YAML is checked with `actionlint` if it is installed, otherwise by review. The first real run is quickstart step 3.

## Risks

- **Repo visibility**: if the repo is private, `latest.json` returns 404 and no app sees updates, and macOS runner minutes are billed. The dry run checks this, and making the repo public is the maintainer's decision before the first tag.
- **Secrets on GitHub**: the signing key lives in repository secrets. Workflows from forks get no secrets, and only `push: tags` triggers the release. Anyone with write access could sign a release, which today means only the maintainer.
- **Quarantine on swap**: research R8 expects none. Quickstart step 3 checks it on the first real update.
- **Lost key**: it needs a manual reinstall on both Macs. The docs ask for a backup.

## Complexity Tracking

No gate violations.
