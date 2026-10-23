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

- **Decision**: The maintainer generates the key once with `npx tauri signer generate -w ~/.tauri/delilah.key` (password protected). The public key goes into `plugins.updater.pubkey`. The private key's contents and its password are stored as the repository secrets `TAURI_SIGNING_PRIVATE_KEY` and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` with `gh secret set`. The maintainer keeps the key file and password backed up (key file plus password manager or Keychain). The local dry run reads the same two variables from the environment, falling back to `~/.tauri/delilah.key` and the Keychain item `delilah-updater`.
- **Rationale**: The release now runs on GitHub (R15), so the key has to be available there. GitHub encrypts secrets at rest, masks them in logs, and doesn't expose them to workflows triggered by forks. FR-019 asks for the key to live outside the repo. Losing the key means a manual reinstall on each Mac, so the docs require a backup.
- **Alternatives**: Signing locally and uploading by hand (that is the manual flow the user doesn't want), or a key without a password (weaker for no gain).

## R5. Build artefacts

- **Decision**: Set `bundle.createUpdaterArtifacts: true`. `tauri build` with the signing env then produces `bundle/macos/Delilah.app.tar.gz` and `Delilah.app.tar.gz.sig` next to the existing `bundle/dmg/Delilah_X.Y.Z_aarch64.dmg`.
- **Rationale**: This is Tauri 2's own switch (v1's `updater.active` is gone). Without the key env, the build fails. That only affects `npm run dist`, so the release script must set the env. Development docs say so, and `tauri dev` and previews are unaffected.
- **Note**: `npm run dist` alone will now fail without the signing variables. Releases run on GitHub (R15), and locally `scripts/release.sh --dry-run` sets them.

## R6. Manifest (`latest.json`)

- **Decision**: Use the static JSON format the updater reads:
  `{ version, notes, pub_date, platforms: { "darwin-aarch64": { signature, url } } }`.
  - `signature` is the *contents* of the `.sig` file.
  - `url` is `https://github.com/FranPerezFolgado/delilah/releases/download/vX.Y.Z/Delilah.app.tar.gz`.
- **Rationale**: This is the documented static-file format and needs no server. It covers Apple Silicon only, as today's builds do.

## R7. Release notes

- **Decision**: A pure Node module `scripts/release/notes.mjs` builds the notes. If the tag is annotated with a non-empty message, the message is used as is. `make release` always annotates the tag with the generated notes, or with `MESSAGE`. Otherwise it turns `git log --format=%s <prev-tag>..<tag>` into Markdown: a "Novedades" section for `feat` and a "Correcciones" section for `fix`, with scope prefixes stripped and merges and other types dropped. The workflow writes the result to a file that is used for both the release body and `latest.json`'s `notes`.
- **Rationale**: The commit-based notes were chosen in clarify. Since nobody edits notes in CI, an annotated tag is the way to write them by hand. Node is on the runner, and `node --test` needs no new dependencies.
- **Alternatives**: `--generate-notes` (it groups by PRs and labels, not conventional commits), or a CHANGELOG file (ruled out in clarify).

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

## R14. Release checks (FR-017)

`scripts/release/check.mjs <version>` is shared by the workflow and the dry run. It exits non-zero with one line per problem:
- the version is the same in the four files (`versions.mjs`) and equals the tag;
- the signing variables are set;
- release `vX.Y.Z` is not already published (looked up with `gh release view`). An existing draft is fine: it is reused, so re-runs are safe.

The dry run also checks, on the Mac, that the tree is clean and that `gh repo view --json visibility` is `PUBLIC`.

## R16. `make release` (manual entry point)

- **Decision**: A root `Makefile` with two thin targets that call Node scripts:
  - `release` runs `node scripts/release/cut.mjs` with `VERSION`, `BUMP` and `YES`.
  - `release-dry-run` runs `scripts/release.sh "$(VERSION)" --dry-run`.
- **`cut.mjs` steps** (stops at the first failure, before anything is pushed):
  1. **Preflight**: the branch is `main`, the tree is clean, `git fetch` succeeds and `main` equals `origin/main` (it never merges by itself), and `gh api user -q .login` is `FranPerezFolgado`, because the machine also has work credentials (CLAUDE.md).
  2. **Pick the version**: `VERSION`, or `bump(current, BUMP)`, or the current version. It must be strictly greater than the latest `v*` tag, and that tag must not exist yet.
  3. If the version changes, it writes it to the four files (`versions.mjs` `writeVersions`), runs `cargo check --workspace`, `cargo check --manifest-path app/src-tauri/Cargo.toml` and `npm install --package-lock-only` in `app/`, then commits `chore: release X.Y.Z` with the four files and three lockfiles.
  4. `node scripts/release/check.mjs X.Y.Z` (the same checks as CI, minus the secrets).
  5. Creates the annotated tag `vX.Y.Z`. Its message comes from `MESSAGE`, else the generated notes, so the notes CI publishes are exactly what the maintainer saw.
  6. Prints `git log origin/main..HEAD` and the tag and asks `¿Publicar vX.Y.Z? [s/N]`, unless `YES=1`.
  7. Runs `git push --atomic origin main vX.Y.Z`. With `--atomic` both refs move or neither does.
  - **On failure after a local commit or tag but before the push**: it undoes its own commit and tag (`git tag -d`, `git reset --hard ORIG`, which is safe because the tree was clean at the start) and says so.
- **Rationale**: The user wants the manual path to be one command. Make is the conventional entry point. The logic is in Node (pure helpers `bump` and `writeVersions`, tested with `node --test`) rather than Make or Bash, so it is testable. It works whether the version was bumped on the feature branch (per CLAUDE.md, the target just tags) or not (the target bumps on `main`).
- **Alternatives**: A Bash script only (harder to test), `cargo release` or `npm version` (each knows only its own files, not the four), or a GitHub "workflow_dispatch" that bumps (it would commit as the bot, not the maintainer).

## R15. Release pipeline (tag push)

- **Decision**: Add `.github/workflows/release.yml`.
  - **Trigger**: `on: push: tags: ['v*.*.*']`. `concurrency: release-${{ github.ref }}`, so two pushes of the same tag never race. `permissions: contents: write`.
  - **Runner**: `macos-14`, which is Apple Silicon and produces `darwin-aarch64`. `actions/checkout` runs with `fetch-depth: 0`, so the notes can see the previous tag.
  - **Steps**:
    1. Set up Node (version from `app/package.json` engines or 22) and Rust (stable, with `aarch64-apple-darwin`). Cache cargo and npm.
    2. Run `node scripts/release/check.mjs`, then the notes.
    3. Run `npm ci` in `app/`.
    4. Run `tauri-apps/tauri-action@v0` with `projectPath: app`, `tagName: ${{ github.ref_name }}`, `releaseName: Delilah ${{ github.ref_name }}`, `releaseBody` = notes, `releaseDraft: true`, `includeUpdaterJson: true`, and the env `GITHUB_TOKEN`, `TAURI_SIGNING_PRIVATE_KEY` and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`. It runs `beforeBuildCommand` (which builds the sidecar `delilahd`), uploads the dmg, `.app.tar.gz` and `.sig`, and writes `latest.json` with `signature` and `url`.
    5. Run `gh release edit vX.Y.Z --draft=false --latest`, only if every step before it passed.
- **Rationale**: The user wants a release to happen when the tag is pushed. `tauri-action` is the maintained way to do the build, upload and `latest.json` for Tauri 2. Its draft mode plus a final publish step gives FR-022: `releases/latest` never resolves to a draft. On a public repo the macOS runner is free.
- **Alternatives**:
  - Our own build and upload in the workflow (it re-implements `latest.json`, which `manifest.mjs` would still cover in the dry run).
  - A local pre-push hook running the script (it only works from this Mac, and the user chose CI).
  - Publishing the draft by hand (the user chose fully automatic).
- **Local dry run**: `scripts/release.sh X.Y.Z --dry-run` runs `check.mjs`, the notes, a signed `npm run dist` and `manifest.mjs` into `app/src-tauri/target/release/bundle/`, and never uploads. Without `--dry-run` it refuses and points to the tag flow.
- **Sidecar**: `scripts/build-sidecar.sh release` builds the workspace `delilahd` for the host triple, so on `macos-14` it is `aarch64-apple-darwin`, as locally.
