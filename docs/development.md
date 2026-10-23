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
- **Previews of tasks.** A project can have a `preview_command` (project view, or `PATCH /api/v1/projects/{name}` with `preview_command`); the daemon runs it in a task's workspace and takes the first URL it prints. For this repo set it to `scripts/preview.sh`: each task's branch then gets its own preview slot, with the UI URL printed first. These nested previews are stopped with the task or by "Parar", never by name. Autopilot starts it before opening the PR.
- A Claude launched from a preview is a real session on the user's subscription. Keep those tests few and short.

**Trying PRs with a fake `gh`.** `DELILAH_GH_BIN=$PWD/scripts/fake-gh.sh` makes the daemon call a script that answers from JSON files in `$FAKE_GH_DIR` (`pr-list.json`, `pr-<n>.json`, `repo.json`, `user.json`), so a PR can be opened, failed, reviewed or merged by editing a file. Steps in [specs/003-pr-loop/quickstart.md](../specs/003-pr-loop/quickstart.md).

## Checks

| Area | Command |
|---|---|
| Rust (workspace) | `cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace` |
| One crate | `cargo test -p delilah-daemon <filter>` |
| Desktop shell | `cargo fmt`, `cargo clippy … -- -D warnings` and `cargo test`, each with `--manifest-path app/src-tauri/Cargo.toml` (it is outside the workspace) |
| UI | `cd app && npx tsc --noEmit && npx vitest run` |

Clippy warnings are errors: the same checks run in three places.

- **Implementers** (`/orchestrate`): `.claude/verify.sh` runs them for the files an agent changed
  (only the touched crates; `vitest related` for the UI) and sends the agent back while they fail.
- **CI** (`.github/workflows/`): `ci.yml` (Rust workspace on macOS, web UI on Linux) on every PR
  and push to `main`; `desktop-shell.yml` only when `app/src-tauri/` changes.
- **You**: the table above.

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

1. Check what is merged into `main` since the last tag (`git log vX.Y.Z..HEAD`).
2. Bump the version: `scripts/bump-version.sh <major|minor|patch|X.Y.Z> [--commit]`. It starts
   from the version on `origin/main`, writes the **four** places (`Cargo.toml` workspace,
   `app/src-tauri/Cargo.toml`, `app/package.json`, `app/src-tauri/tauri.conf.json`) and the
   three lockfiles (only their own version lines), and with `--commit` commits
   `chore: bump version to X.Y.Z`.
3. Commit `chore: release X.Y.Z` and tag `vX.Y.Z`.
4. Build: `cd app && npm run dist` (`CI=true tauri build`, which also builds the
   bundled `delilahd`). The output is
   `app/src-tauri/target/release/bundle/dmg/Delilah_X.Y.Z_aarch64.dmg`.
5. Install: copy `Delilah.app` to `/Applications`, then run
   `xattr -dr com.apple.quarantine /Applications/Delilah.app` (it is not signed).
   Opening it restarts the daemon with the new binary.

## Gotchas

- **Folder trust:** a new workspace path shows Claude's folder-trust dialog once. `run.rs` answers it, but only after it has settled and "Yes" is selected (it used to default to "No, exit").
- **`--resume`** only works when the conversation exists. `has_conversation()` checks first.
- **launchd's environment:** the daemon gets a bare PATH from launchd and asks the login shell for the real one.
- **Fixture submodules over `file://`** need `DELILAH_ALLOW_FILE_PROTOCOL=1`. The dev and preview scripts set it; the installed daemon does not.
- **Notifications in dev builds** are sent as Terminal, and clicking one does not bring the app forward (Tauri reports no clicks).
- **Hot reload:** after changing the shell, restart `tauri dev`. A stale dev app once made ⌘P look broken.
- **Jira:** the site field wants the site root, not a board URL; `site_root()` normalises it, and HTML answers are reported as errors.
- **SpecKit:** it rewrites its block in CLAUDE.md on each plan, which dirties the checkout.

## Tools in this repo

- **QMD:** `delilah` (docs/) and `delilah-specs` (specs/). After adding docs, run `qmd update && qmd embed`.
- **Codegraph:** the code index, in `.codegraph/`, which is git-ignored.
- **SpecKit:** set up in `.specify/`, with its skills in `.claude/skills/` (excluded locally). `/feature` runs `specify → clarify → plan → tasks`.
- **`.claude/launch.json`:** only `preview` (slot 8) for the desktop app's preview tools; nothing that binds 7420.
- **`.claude/settings.json`:** a `PreToolUse` hook (`.claude/hooks/guard-installed-daemon.sh`) that blocks agent
  commands against the installed app: killing or unloading `delilahd`, `scripts/dev-daemon.sh`, writes to the 7420 API,
  the CLI without `DELILAH_URL`, deleting `~/.delilah`. Run those yourself when you really mean it.
- **`.claude/verify.sh`:** the checks the `implementer` agent's stop hook runs (see Checks).
