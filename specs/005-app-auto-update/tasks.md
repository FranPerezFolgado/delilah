# Tasks: App auto-update

**Input**: Design documents from `specs/005-app-auto-update/`
**Prerequisites**: plan.md, spec.md, research.md (R1–R16), data-model.md, contracts/update-contracts.md, quickstart.md

**Tests**: test-first (cargo test, vitest, `node --test`). Write each test task before its implementation and watch it fail.

**Checks**:
- Rust: `cargo test -p delilah-daemon`; shell: `cargo check --manifest-path app/src-tauri/Cargo.toml` (debug) and `cargo check --release --manifest-path app/src-tauri/Cargo.toml` (updater code is release-only).
- App: `cd app && npx tsc --noEmit && npx vitest related --run <files>`.
- Release tooling: `node --test scripts/release/`.
- At the end: `cargo test --workspace`, `cargo test --manifest-path app/src-tauri/Cargo.toml`, `cd app && npx vitest run`, `node --test scripts/release/`.

**Never**:
- try changes against the daemon on 7420; only use `scripts/preview.sh`;
- push, tag on GitHub, set secrets or create releases; `make release` is only tried against a local bare remote;
- generate or commit a signing key. The pubkey is the maintainer's step (T005).

## Format: `[ID] [P?] [Story] Description`

---

## Phase 1: Setup

**Purpose**: dependencies and configuration the stories build on.

- [X] T001 Add `tauri-plugin-updater = "2"` and `tauri-plugin-process = "2"` to `app/src-tauri/Cargo.toml`, pinned with `=` to the current 2.x releases like `notification` and `opener`. Run `cargo check --manifest-path app/src-tauri/Cargo.toml` to refresh `app/src-tauri/Cargo.lock`.
- [X] T002 [P] Add `@tauri-apps/plugin-updater` and `@tauri-apps/plugin-process` to `app/package.json` `dependencies`, at exact versions matching the Rust crates. Run `cd app && npm install` and commit `app/package-lock.json`.
- [X] T003 Edit `app/src-tauri/tauri.conf.json`:
  - set `"bundle.createUpdaterArtifacts": true`;
  - add `"plugins": { "updater": { "pubkey": "REPLACE_WITH_UPDATER_PUBKEY", "endpoints": ["https://github.com/FranPerezFolgado/delilah/releases/latest/download/latest.json"] } }`.
  - CSP stays unchanged (the plugin fetches from Rust).
- [X] T004 [P] Add `"updater:default"` and `"process:allow-restart"` to `app/src-tauri/capabilities/default.json`, and extend its `description` with "checking for, installing and relaunching into updates".
- [ ] T005 **[MAINTAINER, not the agent]** Generate the key and set it up, as in quickstart.md step 1:
  - `npx tauri signer generate -w ~/.tauri/delilah.key`;
  - replace `REPLACE_WITH_UPDATER_PUBKEY` in `tauri.conf.json`;
  - `gh secret set` both secrets as `FranPerezFolgado`;
  - back the key up.
  - The agent leaves the placeholder and lists this step in the PR. `check.mjs` (T022/T024) makes the release fail while the placeholder is there.

**Checkpoint**: `cargo check` (shell) and `npx tsc --noEmit` pass with the new dependencies.

---

## Phase 2: Foundational (blocking all app stories)

**Purpose**: the pure update state machine and the platform boundary.

- [ ] T006 Write `app/src/updates.test.ts` covering every transition in data-model.md "Update state", including:
  - automatic vs manual check outcomes;
  - "Más tarde" sets `dismissed` and an automatic re-check of the same version stays `idle`, but a manual one shows it;
  - an automatic check is ignored while `downloading` / `waiting` / `installing`;
  - an error keeps `version`, and "Reintentar" goes to `downloading`;
  - `meta.lastCheck` / `lastResult` are updated.
  - Fails first.
- [ ] T007 Implement `app/src/updates.ts`: the `UpdateState`, `UpdateMeta` and `UpdateEvent` types and a pure `reduce(state, meta, event, now) → {state, meta}`, exactly per data-model.md. No imports from Tauri or `api.ts`. T006 passes.
- [ ] T008 [P] Extend `app/src/platform.ts` per contracts §2 (dynamic imports, like the existing helpers):
  - `canUpdate` (`isDesktop && !import.meta.env.DEV`);
  - `appVersion()` (`@tauri-apps/api/app` `getVersion`, null outside desktop);
  - `checkForUpdate()`, which returns `{version, notes, install(onProgress)}` or null; `install` uses `downloadAndInstall` and maps `Started`/`Progress` events to `(received, total)`;
  - `relaunch()` (`@tauri-apps/plugin-process`);
  - `onCheckUpdatesMenu(cb)` (`listen("check-updates")`, returns unlisten).
  - Every function is a no-op or returns null when `!canUpdate`.

**Checkpoint**: `npx vitest related --run src/updates.ts` green; `tsc` clean.

---

## Phase 3: User Story 1 — Install a new version from the app (P1) 🎯 MVP

**Goal**: the notice appears for a newer release. "Actualizar" downloads with progress, verifies, installs and relaunches. After the relaunch the daemon, plugin and DB are on the new version, and a pre-upgrade DB copy exists.

**Independent test**:
- vitest: the notice states with a fake platform.
- cargo test: `upgrade.rs`.
- By hand: quickstart "End to end" step 4 (after the first release).

### Tests first

- [ ] T009 [P] [US1] Write `app/src/components/UpdateNotice.test.tsx` with the state passed as props and a `dispatch` spy. It covers:
  - `available` shows "Delilah {version} disponible", the notes rendered with the existing `Markdown` component, and the buttons "Actualizar" and "Más tarde";
  - `downloading` shows "Descargando… {pct}%" (or indeterminate with `total=null`);
  - `installing` shows "Instalando y reiniciando…";
  - `error` shows the reason and "Reintentar" / "Cerrar";
  - `idle` renders nothing.
  - Each button dispatches the right event.
- [ ] T010 [P] [US1] Write tests in a new `crates/delilah-daemon/src/upgrade.rs` (`#[cfg(test)] mod tests`, temp dirs, a real SQLite file created with sqlx). They cover:
  - with no marker and an existing DB, `before_open` writes `backups/delilah-unknown.db`, a valid SQLite copy containing the source rows;
  - with marker `0.4.0` and current `0.5.0`, it writes `backups/delilah-0.4.0.db`;
  - when the marker equals current, there is no copy;
  - an existing copy is never overwritten (its content is preserved);
  - with no DB file (fresh install), there is no copy;
  - with 4 copies, `before_open` prunes to the 3 newest by mtime;
  - `after_open` writes the marker `"0.5.0\n"`;
  - a copy failure (read-only `backups/`) returns `Ok` and logs, without panicking.

### Implementation

- [ ] T011 [US1] Implement `crates/delilah-daemon/src/upgrade.rs` (R10):
  - `pub async fn before_open(home: &Path, db: &Path, current: &str)`: read `home/version`; if it differs and `db` exists, run `VACUUM INTO` through a one-off read-only `SqliteConnectOptions` to `home/backups/delilah-<prev>.db`, creating the dir 0700, never overwriting; then prune to 3. Warn on errors and never fail.
  - `pub fn after_open(home: &Path, current: &str) -> std::io::Result<()>`.
  - Register `mod upgrade;` in `crates/delilah-daemon/src/main.rs` (or `lib.rs` if the crate has one; follow where the other modules live). T010 passes.
- [ ] T012 [US1] Wire it in `crates/delilah-daemon/src/main.rs` `run()`:
  - call `upgrade::before_open(&config.home, &config.database_path(), env!("CARGO_PKG_VERSION")).await` right before `Store::open`;
  - call `upgrade::after_open(...)` right after it succeeds, logging a warning on error.
  - Add one `tracing::info!` when a copy is made.
- [ ] T013 [US1] Implement `app/src/components/UpdateNotice.tsx` as a slim non-blocking banner at the top of the main content, styled in `app/src/styles.css` with the existing tokens and matching the app's other notices. Notes are collapsible ("Ver novedades"). T009 passes.
- [ ] T014 [US1] Implement `app/src/useUpdates.ts`, the hook that owns the side effects. It:
  - does nothing unless `canUpdate`;
  - on mount, reads `appVersion()` into meta and schedules the first automatic check after 10 s and then every 6 h (`setInterval`), clearing both on unmount;
  - `check(manual)` calls `checkForUpdate()` and dispatches the result or error;
  - `update` calls `install()` with progress dispatches, then `relaunch()`;
  - exposes `{state, meta, dispatch, check}`.
  - The session count for US3 is added in T029; until then "Actualizar" goes straight to `downloading`.
- [ ] T015 [US1] Write `app/src/useUpdates.test.ts` with fake timers and a mocked `platform`. It covers:
  - the first check happens at 10 s and the next at 6 h;
  - a found update reaches `available`;
  - `update` calls `install`, then `relaunch`;
  - a failed automatic check stays `idle`;
  - with `canUpdate=false`, nothing is called.
  - (It may be written before T014 and fail first.)
- [ ] T016 [US1] Mount it in `app/src/App.tsx`: call `useUpdates()` once at the top level and render `<UpdateNotice state dispatch />` above the routed views.
- [ ] T017 [US1] Register the plugins in `app/src-tauri/src/main.rs`:
  - `.plugin(tauri_plugin_process::init())` always;
  - `tauri_plugin_updater::Builder::new().build()` only under `#[cfg(not(debug_assertions))]` (R12).
  - Run `cargo check` in debug and `--release` on the shell.
- [ ] T018 [US1] Add the Spanish strings from T009/T013 to `app/src/i18n/en.ts` (English values): "Delilah {version} disponible", "Actualizar", "Más tarde", "Ver novedades", "Descargando… {pct}%", "Instalando y reiniciando…", "No se pudo actualizar: {reason}", "No se pudo verificar la actualización", "Reintentar", "Cerrar".

**Checkpoint**:
- `cargo test -p delilah-daemon upgrade` and `npx vitest related --run src/updates.ts src/useUpdates.ts src/components/UpdateNotice.tsx` are green.
- Quickstart "In a preview" DB-copy steps behave as written.

---

## Phase 4: User Story 2 — Publish a release that installed apps pick up (P1)

**Goal**: `make release` cuts the version and tag on `main`. Pushing tag `vX.Y.Z` makes GitHub check, build, sign and publish the release with `latest.json`, and installed apps are offered it. A local dry run does everything but publish.

**Independent test**:
- `node --test scripts/release/`;
- `make release` in a throwaway clone with a local bare remote (quickstart);
- `actionlint .github/workflows/release.yml` if installed.

### Tests first

- [ ] T019 [P] [US2] `scripts/release/versions.test.mjs`, against a temp copy of the four files:
  - `readVersions(root)` returns `{ "Cargo.toml": "0.4.0", "app/src-tauri/Cargo.toml": …, "app/package.json": …, "app/src-tauri/tauri.conf.json": … }`. It reads only the workspace `[workspace.package]` / `[package]` version, not dependency versions;
  - `mismatches(v, "0.5.0")` lists each file that differs;
  - `writeVersions(root, "0.5.0")` changes only those version fields: the diff is exactly 4 lines and JSON formatting is preserved.
- [ ] T020 [P] [US2] `scripts/release/notes.test.mjs`:
  - `releaseNotes(subjects)` groups `feat(x): a` under "## Novedades" and `fix: b` under "## Correcciones", strips the scope prefixes, drops `docs`/`chore`/`test`/`refactor`/merge subjects, keeps the order, and returns "Mantenimiento interno." when nothing is left;
  - `pickNotes(tagMessage, subjects)` prefers a non-empty tag message (trimmed; the git default "vX.Y.Z" alone counts as empty).
- [ ] T021 [P] [US2] `scripts/release/manifest.test.mjs`: `manifest({version:"0.5.0", notes, pubDate, signature, repo:"FranPerezFolgado/delilah"})` equals the contracts §1 JSON, with `url` `…/releases/download/v0.5.0/Delilah.app.tar.gz`, and a version with a leading `v` is rejected.
- [ ] T022 [P] [US2] `scripts/release/check.test.mjs`. The pure `problems({versions, expected, env, publishedTags, pubkey})` returns one string per failure for:
  - a version mismatch;
  - a non-semver version;
  - a missing `TAURI_SIGNING_PRIVATE_KEY` or `_PASSWORD` (only when `requireSecrets`);
  - an already published tag;
  - the pubkey still `REPLACE_WITH_UPDATER_PUBKEY`.
  - It returns `[]` when all are fine.
- [ ] T023 [P] [US2] `scripts/release/cut.test.mjs`:
  - `bump("0.4.0","minor")` is "0.5.0", with patch, major and invalid levels covered;
  - `pickVersion({current, latestTag, VERSION, BUMP})` handles each row of contracts §3b: tag-only when current is untagged, an error when it is already tagged without BUMP/VERSION, an error when VERSION ≤ latest tag, and an error when both VERSION and BUMP are given.

### Implementation

- [ ] T024 [US2] Implement `scripts/release/versions.mjs`, `notes.mjs`, `manifest.mjs` and `check.mjs` as ESM with only `node:` imports. `check.mjs` exports `problems` and, when run as `node scripts/release/check.mjs X.Y.Z [--require-secrets]`:
  - reads the versions and the pubkey from `tauri.conf.json`;
  - reads published tags with `gh release list --json tagName,isDraft` when `gh` is available (skip with a note otherwise);
  - prints the problems and exits 1 if any.
  - `notes.mjs` CLI `node scripts/release/notes.mjs vX.Y.Z` prints the notes: it reads the tag message with `git tag -l --format='%(contents)' vX.Y.Z` and the subjects since the previous `v*` tag with `git log --format=%s`.
  - T019–T022 pass.
- [ ] T025 [US2] Create `.github/workflows/release.yml` per R15:
  - `on.push.tags: ['v*.*.*']`, `permissions: contents: write`, `concurrency: release-${{ github.ref }}`, `runs-on: macos-14`;
  - steps: checkout (`fetch-depth: 0`); setup-node 22 with npm cache on `app/package-lock.json`; `dtolnay/rust-toolchain@stable`; `Swatinem/rust-cache` (workspaces `. -> target` and `app/src-tauri -> target`); `node scripts/release/check.mjs ${GITHUB_REF_NAME#v} --require-secrets`; `node scripts/release/notes.mjs $GITHUB_REF_NAME > notes.md` (exported as a multiline output); `npm ci` in `app`; `tauri-apps/tauri-action@v0` with `projectPath: app`, `tagName`, `releaseName: Delilah ${{ github.ref_name }}`, `releaseBody` = notes, `releaseDraft: true`, `includeUpdaterJson: true`, and env `GITHUB_TOKEN`, `TAURI_SIGNING_PRIVATE_KEY`, `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`; finally `gh release edit $GITHUB_REF_NAME --draft=false --latest` (env `GH_TOKEN`).
  - Pin third-party actions to a major tag, add a comment at the top with the trigger and the needed secrets, and never echo the secrets.
  - Run `actionlint` on it if it is installed.
- [ ] T026 [US2] Create `scripts/release.sh` (bash, `set -euo pipefail`, like the other scripts):
  - `X.Y.Z --dry-run`:
    1. checks a clean tree and `gh repo view FranPerezFolgado/delilah --json visibility -q .visibility` = `PUBLIC`;
    2. runs `node scripts/release/check.mjs X.Y.Z --require-secrets`, with the env defaulting to `TAURI_SIGNING_PRIVATE_KEY=$(cat ~/.tauri/delilah.key)` and the password from `security find-generic-password -s delilah-updater -w`;
    3. writes the notes and `cd app && npm run dist`;
    4. writes `app/src-tauri/target/release/bundle/latest.json` via `manifest.mjs` from the `.sig` file;
    5. prints the artefact paths.
  - Without `--dry-run`, it exits 2 and prints `make release` / the tag commands. It never uploads.
- [ ] T027 [US2] Implement `scripts/release/cut.mjs` (exports `bump` and `pickVersion`; CLI with env `VERSION`, `BUMP`, `MESSAGE`, `YES`), following R16 steps 1–7:
  - **Preflight**: branch `main`, clean, `git fetch origin`, `HEAD == origin/main`, and `gh api user -q .login` is `FranPerezFolgado`. The `gh` check is skipped only if `DELILAH_RELEASE_SKIP_GH=1`, which is for the local bare-remote test.
  - **Pick the version**.
  - **Bump if needed**: `writeVersions`, the three lockfile refreshes, and `git commit -m "chore: release X.Y.Z"` over exactly the four files and three lockfiles.
  - **Check**: `check.mjs` without secrets.
  - **Tag**: `git tag -a vX.Y.Z -F <notes file>`, with the message from `MESSAGE` or `notes.mjs` over the subjects since the last tag.
  - **Confirm**: print the summary and prompt `¿Publicar vX.Y.Z? [s/N]` unless `YES=1`.
  - **Push**: `git push --atomic origin main vX.Y.Z`.
  - **On any failure after the commit or tag**: delete the tag, `git reset --hard <start sha>`, say so, and exit 1.
  - T023 passes.
- [ ] T028 [US2] Create the root `Makefile`:
  - `.PHONY: release release-dry-run`;
  - `release:` → `node scripts/release/cut.mjs`, passing `VERSION`, `BUMP`, `MESSAGE` and `YES` through the environment;
  - `release-dry-run:` → `scripts/release.sh "$(VERSION)" --dry-run`, failing with usage when `VERSION` is empty.
  - Then try it per quickstart, in the scratchpad: clone, set `origin` to a local bare repo, and use `DELILAH_RELEASE_SKIP_GH=1`.
    - `make release BUMP=patch YES=1` lands the commit and tag in the bare repo.
    - The refusals (dirty tree, off `main`, existing tag) change nothing.
    - Answering `n` undoes the local commit and tag.
  - Record the results in the PR description.

**Checkpoint**:
- `node --test scripts/release/` is green.
- The bare-remote run behaves as in contracts §3b.
- The workflow lints (or is reviewed) and nothing was pushed to GitHub.

---

## Phase 5: User Story 3 — Don't lose running work by surprise (P2)

**Goal**: "Actualizar" with live sessions warns with the count and offers "Actualizar ahora" / "Cuando terminen" / "Cancelar". "Cuando terminen" updates by itself when the count reaches 0.

**Independent test**: vitest with a fake `api.sessions()` and a fake platform. By hand, in the installed app, at the first real update.

- [ ] T029 [US3] Extend `app/src/updates.test.ts` (if T006 didn't already cover them) and add to `app/src/useUpdates.test.ts`:
  - with 2 live sessions, `update` reaches `confirmSessions{running:2}`;
  - `now` reaches `downloading`;
  - `whenIdle` reaches `waiting`, and polling `api.sessions()` (10 s fake timer) updates `running`; when 0, `install` is called;
  - `cancel` returns to `available`;
  - closed or non-live sessions are not counted;
  - a failing `api.sessions()` keeps waiting and doesn't install.
- [ ] T030 [US3] Implement it in `app/src/useUpdates.ts`:
  - on `update`, call `api.sessions()` and count `live && !closed` before dispatching;
  - while `waiting`, poll every 10 s (and on the app's existing live refresh if one is easy to hook into), clearing the poll when leaving `waiting`.
  - T029 passes.
- [ ] T031 [US3] Render `confirmSessions` and `waiting` in `app/src/components/UpdateNotice.tsx`:
  - `confirmSessions`: "Hay {n} sesiones en marcha; se detendrán al actualizar", with `tn` for the singular, and the buttons "Actualizar ahora", "Cuando terminen" and "Cancelar";
  - `waiting`: "Se actualizará cuando terminen {n} sesiones", with "Actualizar ahora" and "Cancelar".
  - Add the cases to `UpdateNotice.test.tsx` and the strings to `app/src/i18n/en.ts`.

**Checkpoint**: vitest green for the three files.

---

## Phase 6: User Story 4 — Check on demand and see the version (P3)

**Goal**: "Buscar actualizaciones…" in the app menu and an "Actualizaciones" section in Settings with the installed version, the last check and "Buscar ahora".

**Independent test**: vitest for the Settings section. By hand, the menu in a release build (shell).

- [ ] T032 [US4] In `app/src-tauri/src/main.rs`:
  - add `const CHECK_UPDATES: &str = "check-updates";` and a menu item `l("Buscar actualizaciones…", "Check for updates…")` in the app menu next to the daemon items;
  - on click, `app.emit_to("main", "check-updates", ())`.
  - The item exists in debug builds too; the UI answers it (T034).
  - Run `cargo check` in debug and release.
- [ ] T033 [P] [US4] Write tests in `app/src/components/SettingsView.test.tsx` (new file, or the existing one if present) for the "Actualizaciones" section, given `updates` props:
  - it shows "Versión {installed}", "Última comprobación: {time}" (or "Todavía no") with its result, and a "Buscar ahora" button that calls `check(true)`;
  - with `canUpdate=false`, it shows "Las actualizaciones solo están en la app instalada" and no button.
- [ ] T034 [US4] Implement it:
  - add the section to `app/src/components/SettingsView.tsx` (new optional prop `updates?: {meta, canUpdate, check}` passed from `App.tsx`);
  - in `useUpdates.ts`, subscribe to `onCheckUpdatesMenu(() => check(true))`;
  - when `!canUpdate` and the menu fires, dispatch an `error` with the "solo en la app instalada" text so dev builds answer.
  - Render `checking` (manual) and `upToDate` ("Delilah está al día ({version})") in `UpdateNotice.tsx`.
  - Add the strings to `app/src/i18n/en.ts`.
  - T033 passes.

**Checkpoint**: vitest green; the shell `cargo check` passes in both profiles.

---

## Phase 7: Polish & cross-cutting

- [ ] T035 [P] Rewrite `docs/development.md` "Releasing":
  - bump on the branch per CLAUDE.md, merge, then `make release` on `main` (or `BUMP=` / `VERSION=` / `MESSAGE=`), and what the workflow does;
  - `make release-dry-run`;
  - one-time setup: key generation, `gh secret set` both secrets as `FranPerezFolgado`, a key backup and why, and making sure the repo is public;
  - bootstrap: install 0.5.0 by hand from the release's dmg, once per Mac;
  - that `npm run dist` now needs the signing env;
  - `$DELILAH_HOME/backups/` and how to restore a copy by hand.
  - Add a Gotchas bullet: no updater in dev builds.
- [ ] T036 [P] Update `docs/architecture.md`: an "Updates" subsection (who checks, the state machine in `updates.ts`, the relaunch → existing daemon restart → plugin rewrite → migrations), the new files `version` and `backups/` in the data section, and `.github/workflows/release.yml` and `Makefile` in the layout.
- [ ] T037 [P] Add decisions to `docs/decisions.md`, dated 2026-10-24 and numbered after the last one:
  - Tauri updater with GitHub Releases on the public repo;
  - user-confirmed updates with the session warning ("Cuando terminen");
  - release on tag push in GitHub Actions, the key in repository secrets, and `make release` as the manual entry point;
  - no Apple signing or notarisation for now;
  - the best-effort DB copy before migrations.
  - Include what was ruled out (Sparkle, a private repo with a token, a local-only script, a silent update).
- [ ] T038 [P] Update `docs/status.md` (built: auto-update; pending: the maintainer's T005 and the first tagged release) and `README.md` (a section "Actualizaciones": the notice, "Cuando terminen", the menu item, Settings, and that dev builds don't update).
- [ ] T039 [P] In `CLAUDE.md` "Versioning", change "Tagging and building stay as in the release steps" to point at `make release` and the tag workflow in docs/development.md. The per-branch bump rule stays.
- [ ] T040 Version bump, **minor** (new user-facing feature): re-check the version on `main` (`git show main:Cargo.toml | grep ^version`), then bump from it, 0.4.0 → 0.5.0 if unchanged. Edit the four files (use `writeVersions` from `scripts/release/versions.mjs`), refresh the lockfiles (`cargo check --workspace`, `cargo check --manifest-path app/src-tauri/Cargo.toml`, `cd app && npm install --package-lock-only`), and commit them on their own as `chore: bump version to 0.5.0`.
- [ ] T041 Full checks: `cargo test --workspace`, `cargo test --manifest-path app/src-tauri/Cargo.toml`, `cargo check --release --manifest-path app/src-tauri/Cargo.toml`, `cd app && npx tsc --noEmit && npx vitest run`, `node --test scripts/release/`. Run quickstart "In a preview" (Settings section, DB copies with the fake `version` marker) with `scripts/preview.sh --fresh` in the background, and stop it by its process, never by name.
- [ ] T042 PR description:
  - the version level (minor, and why);
  - the maintainer's steps (T005, making the repo public, the first `make release` and the bootstrap install);
  - what couldn't be tried before a real tag (the workflow run, the in-app update, quarantine after the swap: quickstart steps 4–5).

---

## Dependencies & execution order

- **Phase 1 → Phase 2 → US1** (MVP). T005 is the maintainer's and blocks nothing in code. It only blocks the first real release (T024's check refuses the placeholder).
- **US2** (T019–T028) depends only on Phase 1 (T003 for the pubkey check) and can run in parallel with Phase 2 and US1: its files are all in `scripts/`, `.github/` and `Makefile`.
- **US3** depends on US1 (T014, T013). **US4** depends on US1 (T014, T016) and touches `main.rs` after T017.
- **Polish** comes last. T040 comes after all code. T041 comes after T040.

### Parallel opportunities

- T001 ∥ T002 ∥ T004. T008 ∥ T006/T007.
- In US1: T009 ∥ T010 (app vs daemon). T011/T012 (daemon) ∥ T013–T016 (app).
- US2 as a whole ∥ Phase 2 + US1. Inside US2: T019–T023 are all [P].
- T035–T039 are all [P].

## Implementation strategy

1. **MVP**: Phases 1–3 plus US2 (both P1). Without a release pipeline there is nothing to install, so the first useful slice is US1 + US2.
2. Then US3 (session safety, P2) before the first real release. Don't tag 0.5.0 without it.
3. Then US4, polish, the bump and the PR. The maintainer then does T005 and `make release`.
