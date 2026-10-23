# Working on Delilah

The practical guide: how to run it, test it, change it and release it. Read
[architecture.md](architecture.md) first for how it fits together.

## The three Delilahs on this Mac

| Instance | Daemon | State | Use it for |
|---|---|---|---|
| Installed app (`/Applications/Delilah.app`) | 7420, run by launchd | `~/.delilah` | The user's real work, **including the agents building Delilah**. Never restart or stop it to try a change. |
| Preview (`scripts/preview.sh`) | 7501–7509, UI 1501–1509 | `fixtures/preview-home-N` | **Trying any change.** One per checkout or worktree. |
| Dev (`scripts/dev-daemon.sh` + `npm run tauri dev`) | 7420 | `fixtures/dev-home` | Native-only work (menu, notifications, Dock), and only with the installed app stopped by the user. |

```bash
scripts/preview.sh            # build this checkout, start daemon + UI, print the URLs
scripts/preview.sh --fresh    # empty state
scripts/preview.sh --no-ui    # API only: DELILAH_URL=http://127.0.0.1:750N target/debug/delilah ls
```

How to use a preview:
- Run it in the background and stop it with Ctrl-C, or by stopping that process. **Never stop a daemon by name** (`pkill delilahd`): it also kills the user's live agent sessions.
- Open the UI in a browser. In a preview, register the fixture repos (`fixtures/parent-env/parent` as `parent`, `fixtures/signal` as `repo`) as projects.
- A Claude launched from a preview is a real session on the user's subscription. Keep those tests few and short.

**Trying PRs with a fake `gh`.** `DELILAH_GH_BIN=$PWD/scripts/fake-gh.sh` makes the daemon call a script that answers from JSON files in `$FAKE_GH_DIR` (`pr-list.json`, `pr-<n>.json`, `repo.json`, `user.json`), so a PR can be opened, failed, reviewed or merged by editing a file. Steps in [specs/003-pr-loop/quickstart.md](../specs/003-pr-loop/quickstart.md).

## Checks

| Area | Command |
|---|---|
| Rust (workspace) | `cargo fmt --all && cargo clippy --workspace --all-targets && cargo test --workspace` |
| One crate | `cargo test -p delilah-daemon <filter>` |
| Desktop shell | `cargo test --manifest-path app/src-tauri/Cargo.toml` (it is outside the workspace) |
| UI | `cd app && npx tsc --noEmit && npx vitest run` |

The git tests build throwaway repos and need `git -c protocol.file.allow=always`;
the fixtures use it already.

## Conventions

- **Comments** explain why, not what. Match the existing density and tone: short, plain English, the reason for a choice or a gotcha.
- **API changes** touch both sides: the route and handler in `crates/delilah-daemon/src/*.rs`, and the type and call in `app/src/api.ts` (and `crates/delilah-cli` if the CLI uses it). The serde shapes and the TS types must match.
- **Migrations** are a new `crates/delilah-daemon/migrations/NNNN_name.sql`. Never edit one that has shipped, and keep them additive (existing databases are migrated in place). A migration renumbered by a merge (as `0011_idea_conversations` → `0014`) breaks a preview home that already applied the old number: start that preview with `--fresh`.
- **Status changes** go through `AppState::transition` (`transitions.rs`), so events and notifications fire. **Phase changes** go through `phases::advance` (forward only) or `phases::set` (the user).
- **Events:** after a change a client must see, emit the right `Topic`.
- **UI texts** are Spanish inside `t()` / `tn()`, with the English in `app/src/i18n/en.ts`. The daemon picks its texts with `app.lang().pick(es, en)`.
- **Tests:** each behaviour gets a test, named as a sentence (`signals_only_move_forward_but_the_user_moves_anywhere`). Daemon tests use `Store::in_memory()`, a `Config` with `notify: false`, and the in-memory secrets.
- **Agents' hooks:** only in the settings file under `tasks/<KEY>/` (`claude::write_settings`). Agents get the clean environment of `delilah-agents::env`.
- **Nothing from the work repos**, ever. Use the fixtures and the Jira sample data.

## Git

- **Account:** the personal one (FranPerezFolgado, `franperezfolgado@outlook.com`). Confirm it before any remote operation: the machine also has work credentials.
- **Commit, don't push:** commit finished work without asking; the user pushes. No `Co-Authored-By: Claude` trailer.
- **Others' changes:** don't commit changes you did not make (e.g. SpecKit's CLAUDE.md block, which points at whichever feature is current). Say so instead.
- **Checkout mode needs a clean checkout.** Anything left uncommitted in the main checkout blocks new tasks there.

## Releasing

A release is a tag `vX.Y.Z` on `main`; the GitHub Actions workflow
(`.github/workflows/release.yml`) builds, signs and publishes it, and the installed
apps pick it up (see Updates in [architecture.md](architecture.md#updates)).

1. **On the feature branch**, bump the version as CLAUDE.md says (its own commit
   `chore: bump version to X.Y.Z`) in **four** places and refresh the lockfiles:
   - `Cargo.toml` (workspace)
   - `app/src-tauri/Cargo.toml`
   - `app/package.json`
   - `app/src-tauri/tauri.conf.json`
2. Merge the PR into `main`, then `git switch main && git pull`.
3. `make release` on `main`. The version comes from `BUMP=major|minor|patch` or `VERSION=X.Y.Z`; notes (`MESSAGE="…"`; `YES=1` skips the confirmation). It
   checks that you are on `main`, level with `origin/main` and clean, bumps the
   four files only if they are not already at that version (refreshing the
   lockfiles with `cargo metadata` and `npm install --package-lock-only`), commits
   `chore: release X.Y.Z`, creates the annotated tag `vX.Y.Z` (its message is the
   release notes) and pushes `main` and the tag together.
   Variables are exported to `scripts/release/cut.mjs`, never interpolated.
   Its logic is tested with `node --test scripts/release/*.test.mjs` (the bare
   directory form does not work on Node 22).
4. **What the workflow does** on a `v*.*.*` tag: checks the commit is on `main`,
   fetches the annotated tag (`git fetch --force --tags`), runs
   `scripts/release/check.mjs --require-secrets` (versions agree, the public key is
   not the `REPLACE_WITH_UPDATER_PUBKEY` placeholder, the secrets exist), writes the
   notes (the tag message, else the `feat`/`fix` subjects since the last tag),
   builds with tauri-action and uploads a **draft** release (`includeUpdaterJson:
   false`), builds `latest.json` itself with `manifest.mjs --write` from the local
   `.sig`, checks the `Delilah.app.tar.gz` asset, uploads with `--clobber` and only
   then publishes it (`gh release edit --draft=false --latest`). A failed run leaves
   at most a draft; re-running the same tag is safe. tauri-action's own
   `latest.json` is not used: it carries temporary `untagged-*` draft URLs (decision 39).
5. Installed apps check for it within 6 h (or at once with "Buscar actualizaciones…").

**Rehearsal:** `make release-dry-run VERSION=X.Y.Z` (`scripts/release.sh`) runs the
checks, builds and signs the app and writes `latest.json` locally. It never uploads.
To try `make release` itself against a local bare remote, set
`DELILAH_RELEASE_SKIP_GH=1` and `DELILAH_RELEASE_SKIP_LOCK=1` (skip the `gh` checks
and the lockfile refresh; for tests only).

**One-time setup:**
1. Make sure the repo is public (the updater downloads from the release without a token).
2. Generate the key: `cd app && npx tauri signer generate -w ~/.tauri/delilah.key`,
   and paste the public key into `plugins.updater.pubkey` in `tauri.conf.json`
   (until then it holds `REPLACE_WITH_UPDATER_PUBKEY` and `check.mjs` refuses to release).
3. As `FranPerezFolgado`: `gh secret set TAURI_SIGNING_PRIVATE_KEY < ~/.tauri/delilah.key`
   and `gh secret set TAURI_SIGNING_PRIVATE_KEY_PASSWORD`.
4. **Back up the key file and its password** somewhere outside this Mac. An app only
   accepts updates signed with the key whose public half it shipped with: if the key
   is lost, no installed Delilah can ever update again and each Mac needs a manual
   reinstall with a new key.

**Bootstrap:** the updater only exists from the first release with it. Install that
release (0.5.0) by hand, once per Mac: download the `.dmg` from the GitHub release,
copy `Delilah.app` to `/Applications` and run
`xattr -dr com.apple.quarantine /Applications/Delilah.app` (it is not signed with an
Apple identity). From then on it updates itself.

**Local builds:** `cd app && npm run dist` (`CI=true tauri build`, which also builds
the bundled `delilahd`) now needs the signing env, because the bundle config creates
update artefacts: export `TAURI_SIGNING_PRIVATE_KEY` (the key's contents or path) and
`TAURI_SIGNING_PRIVATE_KEY_PASSWORD` first. The output is
`app/src-tauri/target/release/bundle/dmg/Delilah_X.Y.Z_aarch64.dmg`.

**Backups and restoring:** when the daemon starts on a different version than
`$DELILAH_HOME/version` says (and the database exists), it copies the database to
`$DELILAH_HOME/backups/delilah-<previous version>.db` before migrating (`VACUUM INTO`;
never overwrites; keeps the 3 newest; a failure only logs a warning). To go back:
quit Delilah (Delilah → Detener el daemon), move `delilah.db` (and any `-wal`/`-shm`)
aside, copy the backup to `$DELILAH_HOME/delilah.db`, install the older version and
open it. A downgrade also makes a copy, named after the version in the marker.

## Gotchas

- **Folder trust:** a new workspace path shows Claude's folder-trust dialog once. `run.rs` answers it, but only after it has settled and "Yes" is selected (it used to default to "No, exit").
- **`--resume`** only works when the conversation exists. `has_conversation()` checks first.
- **launchd's environment:** the daemon gets a bare PATH from launchd and asks the login shell for the real one.
- **Fixture submodules over `file://`** need `DELILAH_ALLOW_FILE_PROTOCOL=1`. The dev and preview scripts set it; the installed daemon does not.
- **Notifications in dev builds** are sent as Terminal, and clicking one does not bring the app forward (Tauri reports no clicks).
- **Hot reload:** after changing the shell, restart `tauri dev`. A stale dev app once made ⌘P look broken.
- **No updater in dev builds:** it runs only in release builds (`cfg(not(debug_assertions))` in the shell, `!import.meta.env.DEV` in the UI), so `tauri dev` and previews never check for or offer updates.
- **Jira:** the site field wants the site root, not a board URL; `site_root()` normalises it, and HTML answers are reported as errors.
- **SpecKit:** it rewrites its block in CLAUDE.md on each plan, which dirties the checkout.

## Tools in this repo

- **QMD:** `delilah` (docs/) and `delilah-specs` (specs/). After adding docs, run `qmd update && qmd embed`.
- **Codegraph:** the code index, in `.codegraph/`, which is git-ignored.
- **SpecKit:** set up in `.specify/`, with its skills in `.claude/skills/` (excluded locally). `/feature` runs `specify → clarify → plan → tasks`.
- **`.claude/launch.json`:** `daemon`, `ui` and `preview` (slot 8) for the desktop app's preview tools.
