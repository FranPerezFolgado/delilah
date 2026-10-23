# Research: App auto-update

## R1. Update mechanism

- **Decision**: Use Tauri 2's official `tauri-plugin-updater` (Rust) with `@tauri-apps/plugin-updater` (JS), and `tauri-plugin-process` / `@tauri-apps/plugin-process` for `relaunch()`.
- **Rationale**: It is maintained alongside Tauri 2 (already the shell). It verifies minisign signatures against a public key embedded in `tauri.conf.json`, downloads the update with progress events, swaps `Delilah.app` in place on macOS (the `.app.tar.gz` artefact), and refuses equal or older versions by default (semver comparison). That covers FR-003, FR-010, FR-011 and SC-004 without our own code.
- **Alternatives**: Sparkle (native, but means Objective-C/Swift glue and its own appcast and EdDSA key on top of Tauri), a hand-written download-and-replace (re-implements signature checks and the atomic swap), Homebrew cask (needs a tap, `brew upgrade` by hand, so it doesn't meet "from the app").

## R2. Where the update logic lives

- **Decision**: Put the check/download/install flow in the web UI through the JS plugin API, wrapped in `app/src/platform.ts`, with the state machine as a pure reducer in `app/src/updates.ts`. The Rust shell only registers the plugins and adds a menu item that emits an event to the window.
- **Rationale**: `platform.ts` is already the one place for "things only the desktop shell can do" (decision on keeping another shell possible). The UI already has live session data (needed for "Cuando terminen") and the notice/Settings UI. A pure reducer can be tested with vitest; the Rust side stays a few lines.
- **Alternatives**: Do it in Rust and push state to the UI with events. That duplicates the session list in the shell and is harder to test (no vitest, and Tauri apps aren't covered by `cargo test` in a useful way).

## R3. Hosting and endpoint

- **Decision**: Use GitHub Releases on the public `FranPerezFolgado/delilah`, with endpoint `https://github.com/FranPerezFolgado/delilah/releases/latest/download/latest.json`.
- **Rationale**: Chosen in clarify. `releases/latest` only resolves to published (non-draft, non-prerelease) releases, so a draft is invisible to installed apps. That gives FR-022 (manifest seen only once everything is uploaded) for free, as long as the release is created as a draft and published last.
- **Alternatives**: A private repo plus a token (ruled out in clarify), or a separate releases repo (not needed if the main repo is public).

## R4. Signing key

- **Decision**: Generate the key with `npx tauri signer generate -w ~/.tauri/delilah.key` (password protected), done once by the maintainer. The public key goes into `plugins.updater.pubkey`. The release script exports `TAURI_SIGNING_PRIVATE_KEY` (path or content) and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`; the password is read from the macOS Keychain (`security find-generic-password -s delilah-updater -w`) so it is never in the repo or shell history.
- **Rationale**: This is the standard Tauri flow, and FR-019 asks for the key to live outside the repo. The Keychain is already on the Mac and is backed up with it. The docs must say to back the key file up, because losing it means a manual reinstall on every Mac (spec Edge Cases).
- **Alternatives**: A key without a password (weaker for no gain), or the password in an env file (easy to leak).

## R5. Build artefacts

- **Decision**: Set `bundle.createUpdaterArtifacts: true`. `tauri build` with the signing env then produces `bundle/macos/Delilah.app.tar.gz` and `Delilah.app.tar.gz.sig` next to the existing `bundle/dmg/Delilah_X.Y.Z_aarch64.dmg`.
- **Rationale**: This is Tauri 2's own switch (v1's `updater.active` is gone). Without the key env, the build fails. That only affects `npm run dist`, so the release script must set the env. Development docs say so, and `tauri dev` and previews are unaffected.
- **Note**: `npm run dist` alone (without the script) will now fail without the key; the release docs replace it with `scripts/release.sh`.

## R6. Manifest (`latest.json`)

- **Decision**: Use the static JSON format the updater reads:
  `{ version, notes, pub_date, platforms: { "darwin-aarch64": { signature, url } } }`.
  - `signature` is the *contents* of the `.sig` file.
  - `url` is `https://github.com/FranPerezFolgado/delilah/releases/download/vX.Y.Z/Delilah.app.tar.gz`.
- **Rationale**: This is the documented static-file format and needs no server. It covers Apple Silicon only, as today's builds do.

## R7. Release notes

- **Decision**: A pure Node module `scripts/release/notes.mjs` turns `git log --format=%s <prev-tag>..<tag>` into Markdown: a "Novedades" section for `feat` and a "Correcciones" section for `fix`. Scope prefixes are stripped and merges and other types are dropped. The script writes it to a temp file, opens `$EDITOR` (skipped with `--yes`), and passes it to both `gh release create --notes-file` and `latest.json`'s `notes`.
- **Rationale**: Chosen in clarify. Node already ships with the app toolchain, and `node --test` needs no new dependencies.
- **Alternatives**: `gh release create --generate-notes` (it groups by PRs and labels rather than conventional commits, and isn't available to embed in `latest.json` before creation).

## R8. Quarantine

- **Decision**: No action needed. The updater downloads with its own HTTP client, not a browser, so the replacement bundle carries no `com.apple.quarantine` attribute. Gatekeeper therefore doesn't block the unsigned relaunched app (FR-015). The first install of the updater-capable version is still done by hand (bootstrap).
- **Risk**: If a future macOS starts checking the replacement, the fix is Apple signing, which is out of scope. The quickstart has a check step for this.

## R9. Daemon restart, plugin and migrations after the swap

- **Decision**: Reuse what exists. On relaunch, `daemon::ensure_running` (`app/src-tauri/src/daemon.rs:34-37`) sees that the bundled `delilahd` mtime differs and restarts the LaunchAgent. The new daemon rewrites the plugin with its `CARGO_PKG_VERSION` (`crates/delilah-daemon/src/main.rs:42`) and runs `sqlx::migrate!` in `Store::connect`.
- **Rationale**: This already works for manual installs. The extraction from `tar.gz` keeps the build's mtime, which differs from the old build's.

## R10. Pre-upgrade database copy (FR-014)

- **Decision**: Add a new `crates/delilah-daemon/src/upgrade.rs`. It runs in `run()` before `Store::open` and uses a marker file at `$DELILAH_HOME/version` that holds the version that last opened the database.
  - If the marker differs from `CARGO_PKG_VERSION` (or is missing) and `delilah.db` exists, it opens the DB once read-only with sqlx and runs `VACUUM INTO '$DELILAH_HOME/backups/delilah-<prev>.db'`. `<prev>` is the marker's content or `unknown`.
  - It never overwrites an existing copy, so a failed upgrade retried at the next start doesn't overwrite the true pre-upgrade copy.
  - It prunes to the 3 newest copies.
  - It writes the marker only after `Store::open` (migrations) succeeds.
  - If the copy fails, it logs a warning and carries on, because a full disk mustn't brick the daemon. The edge-case wording in the spec ("keeps a copy") is best-effort, and that is noted in decisions.
- **Rationale**: `VACUUM INTO` gives a consistent copy even with a WAL. A plain file copy can miss `-wal` content.
- **Alternatives**: Copying `delilah.db` along with `-wal`/`-shm` (fragile), or a `meta` table in the DB (it would need a migration, and the copy has to happen *before* migrations anyway).

## R11. Counting running sessions (FR-009)

- **Decision**: Use `GET /api/v1/sessions` (already in `api.ts`) and count sessions with `live === true && !closed`. That covers Claude and shell sessions, project and task sessions, working or waiting. "Cuando terminen" re-reads the count on the UI's existing live refresh (or every 10 s), and starts the install when it reaches 0.
- **Rationale**: `live` is "its process is running now", which is exactly what a daemon restart ends. No API change is needed.

## R12. Disabling in development

- **Decision**: Register `tauri-plugin-updater` only in release builds (`#[cfg(not(debug_assertions))]`). In the UI, `canUpdate = isDesktop && !import.meta.env.DEV`. Previews run in a plain browser (`isDesktop === false`).
- **Rationale**: FR-005. A `tauri dev` build must never replace itself with the installed release.

## R13. Check cadence

- **Decision**: Check 10 s after start (so the launch isn't slowed and the daemon is up), then every 6 h with `setInterval`, and on demand from the menu ("Buscar actualizaciones…", which emits the `check-updates` event) and from Settings. "Más tarde" stores the dismissed version in memory only.
- **Rationale**: FR-001, FR-002 and FR-008.

## R14. Release script checks (FR-017)

`scripts/release.sh vX.Y.Z` stops before building if any of these fail:
- the working tree is clean and on `main`;
- tag `vX.Y.Z` exists and points at `HEAD`;
- the version matches in the four files (pure check in `scripts/release/versions.mjs`);
- the key file exists and the Keychain password is readable;
- `gh auth status` reports the active account `FranPerezFolgado` (the work credentials must not be used);
- `gh repo view --json visibility` is `PUBLIC`;
- no release `vX.Y.Z` is already published. An existing draft is reused, so it is safe to run again.

Publishing order:
1. `gh release create --draft` (or reuse the draft).
2. Upload the dmg, `.app.tar.gz` and `.sig` with `--clobber`.
3. Upload `latest.json`.
4. `gh release edit --draft=false --latest`.

Any failure leaves a draft (FR-022).
