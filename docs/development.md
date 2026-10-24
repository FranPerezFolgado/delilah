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
| UI | `cd app && npx tsc --noEmit && npx tsc --noEmit -p e2e && npx vitest run` (the second `tsc` checks the Playwright specs) |

Clippy warnings are errors: the same checks run in three places.

- **Implementers** (`/orchestrate`): `.claude/verify.sh` runs them for the files an agent changed
  (only the touched crates; `vitest related` for the UI) and sends the agent back while they fail.
- **CI** (`.github/workflows/`): `ci.yml` (Rust workspace on macOS, web UI on Linux) on every PR
  and push to `main`; `desktop-shell.yml` only when `app/src-tauri/` changes.
- **You**: the table above.

The git tests build throwaway repos and need `git -c protocol.file.allow=always`;
the fixtures use it already.

## End-to-end tests

Two layers drive a real `delilahd` against a **fake Claude** (and a fake `gh` and Jira):
- **API/CLI** (`crates/delilah-e2e/tests/*.rs`, behind the `e2e` feature, so `cargo test --workspace` does not run them): 11 flows through HTTP and the `delilah` binary.
- **UI** (`app/e2e/*.spec.ts`, Playwright, Chromium headless): 6 flows (plus a smoke check) in the browser, against one daemon and one Vite per run, tests serial.

```bash
scripts/e2e.sh                  # both layers: API/CLI, then UI
scripts/e2e.sh api              # or: ui
scripts/e2e.sh api -- <filter>  # only the tests whose name matches
scripts/e2e.sh --real-claude    # the smoke test, see below
```

It builds the daemon, the CLI and the harness first, and ends with `api: PASS (Ns)` / `ui: FAIL (Ns)` (non-zero exit if any failed). The first UI run installs Chromium. On a failing UI test, `app/playwright-report/` has the screenshot, trace and daemon log.

- **Safe with the installed Delilah running.** Each test gets a temp root with its own `HOME`, `DELILAH_HOME`, fixture repos and a free port (never 7420 nor 7501-7509), and the daemon runs with `env -i`. A tripwire `claude`/`gh` first on PATH fails the test if either real binary is ever reached. The daemon's one-shot Claude calls (`DELILAH_CLAUDE_BIN`: branch name, submodule suggestion) go to `bin/claude-oneshot`, which logs to `<root>/oneshot.log` and fails, so branches get the title-based name (`feature/<KEY>-<title-slug>`). Nothing touches `~/.delilah`. Never kill a daemon by name here either.
- **Run it before a PR that touches the daemon, the CLI or the UI.** Local only: there is no CI.
- `DELILAH_E2E_KEEP=1` keeps the temp roots of failed tests (the path is printed). `target/debug/e2e-env start --jira --keep` starts a debug environment: use its printed `api` with `DELILAH_URL=<api> target/debug/delilah ls`, then Ctrl-D.
- Waits are bounded (`wait_until`, 15 s by default, 30 s at most): fix flakiness by waiting for the condition, never by raising a timeout.

**Adding an API flow.** A `#[test]` in `crates/delilah-e2e/tests/<area>.rs`:

```rust
use std::time::Duration;
use delilah_e2e::api::items;
use delilah_e2e::{Env, EnvOptions};
use serde_json::json;

const T: Duration = Duration::from_secs(20);

let env = Env::start(EnvOptions::default());
// An option is a string or {label, description?, preview?}; `header` is required.
env.scenario("ask", &json!({"steps": [
    {"ask": {"question": "Which colour?", "header": "Colour", "options": ["red", "blue"]}},
    {"stop": "done"},
]}));
env.register_fixtures();
env.api().start_run(json!({
    "key": "E2E-1", "title": "Ask", "project": "signal", "prompt": "Ask [fake:ask]",
}));
let ask = env.wait_until("the ask in the inbox", T, || {
    let inbox = env.api().inbox();
    items(&inbox).iter().find(|i| i["ask"].is_object()).map(|i| i["ask"].clone())
});
env.api().answer_ask(ask["id"].as_str().unwrap(), json!({"Which colour?": "blue"}));
env.wait_until("the fake got the answer", T, || {
    env.record_for("E2E-1").map(|r| r.answers).filter(|a| !a.is_empty())
});
```

`wait_until`/`wait_for` take `(what, timeout, f)`; `api().inbox()` is a JSON `Value` (`items()` gives its list); `record_for(key)` is an `Option<Record>`. See `crates/delilah-e2e/tests/states.rs` for the full version.

**Adding a UI flow.** A new `app/e2e/<flow>.spec.ts` importing `test` and `expect` from `./fixtures` (not from Playwright), as the existing specs do. Besides `page`, the fixtures are `api`, `env`, `fixture()`, `scenario(name, {steps: [...]})` and `project("repo" | "parent")`, which copies a fixture repo and registers it as a project of its own (removed after the test). Each spec pins its locale with `test.use({ locale: … })` (for example `"en-GB"`), because the UI text follows the browser language.

```ts
import { expect, test } from "./fixtures";

test.use({ locale: "en-GB" });

test("my flow", async ({ page, project, scenario }) => {
  scenario("ask", { steps: [{ ask: { question: "Which colour?", header: "Colour", options: ["red", "blue"] } }, { stop: "done" }] });
  const p = await project("repo");
  // create a task whose title ends in [fake:ask] in project p.name, then drive the page
});
```

**Scenarios.** `env.scenario(name, json)` writes `<root>/fake/scenarios/<name>.json`; a task picks one with `[fake:<name>]` in its prompt, else `<task>.json`, else `default.json` (stop, then wait for input). After the last step the fake waits for input like an idle Claude. What each fake session saw (args, hooks, answers, inputs) is in `<root>/fake/records/<session>.json` (`env.record_for`). An unknown step makes the fake run `StopFailure` and exit 2.

| Step | Effect |
|---|---|
| `trust_dialog` | prints the folder-trust dialog and waits for Enter (a leading one comes before `SessionStart`, as in Claude) |
| `print` | writes text to the terminal |
| `hook` | runs the settings' hook for that event with a minimal payload |
| `sleep_ms` | waits (at most 5000) |
| `ask` | runs the `AskUserQuestion` `PreToolUse` hook, records the answer, then `PostToolUse`. Each option is a string or `{label, description?, preview?}` (`Nueva tarea` + `Crear` with a `preview` is the task-from-idea card) |
| `stop` | runs the `Stop` hook with the message |
| `wait_input` | reads one line from the terminal, records it, runs `UserPromptSubmit` |
| `fail` | prints the message and runs `StopFailure`, then stays alive reading input (later steps are skipped; the error item stays in the inbox until the session ends) |
| `exit` | runs `SessionEnd` and exits with the code |

**The smoke test.** `scripts/e2e.sh --real-claude` runs one real Claude Code session (a task "Reply OK and stop.", wait for working then waiting, close, archive). It needs `DELILAH_E2E_REAL_CLAUDE=1` (the script sets it), uses the user's real `HOME` (still its own `DELILAH_HOME` and port, no tripwire) and **spends subscription usage**, so it is opt-in and never part of the default run.

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

1. **On the feature branch**, bump the version as CLAUDE.md says, in its own commit
   `chore: bump version to X.Y.Z`: `scripts/bump-version.sh <major|minor|patch|X.Y.Z> [--commit]`.
   It starts from the version on `origin/main`, writes the **four** places (`Cargo.toml`
   workspace, `app/src-tauri/Cargo.toml`, `app/package.json`,
   `app/src-tauri/tauri.conf.json`) and the three lockfiles (only their own version
   lines), and with `--commit` makes the commit. Check what is merged since the last tag
   with `git log vX.Y.Z..HEAD`.
2. Merge the PR into `main`, then `git switch main && git pull`.
3. `make release` on `main`. With neither `BUMP` nor `VERSION` it releases the version
   already in the files, which is the normal path after the branch bump. Use
   `BUMP=major|minor|patch` or `VERSION=X.Y.Z` only when the branch was not bumped (see
   `pickVersion` in `scripts/release/cut.mjs`). Notes: `MESSAGE="…"`; `YES=1` skips the
   confirmation. It checks that `gh` is logged in as `FranPerezFolgado`, that you are on
   `main`, level with `origin/main` and clean, bumps the four files only if they are not
   already at that version (refreshing the lockfiles with `cargo metadata` and
   `npm install --package-lock-only`), commits `chore: release X.Y.Z`, creates the
   annotated tag `vX.Y.Z` (its message is the release notes) and pushes `main` and the
   tag together.
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
   `latest.json` is not used: it carries temporary `untagged-*` draft URLs (decision 62).
5. Installed apps check for it within 6 h (or at once with "Buscar actualizaciones…").

**Rehearsal:** `make release-dry-run VERSION=X.Y.Z` (`scripts/release.sh`) runs the
checks, builds and signs the app and writes `latest.json` locally. It never uploads.
It needs the repo to be public, the key at `~/.tauri/delilah.key` and its password in
the Keychain item `delilah-updater` (or `TAURI_SIGNING_PRIVATE_KEY` and
`TAURI_SIGNING_PRIVATE_KEY_PASSWORD` set in the environment).
To try `make release` itself against a local bare remote, set
`DELILAH_RELEASE_SKIP_GH=1` and `DELILAH_RELEASE_SKIP_LOCK=1` (skip the `gh` checks
and the lockfile refresh; for tests only).

**One-time setup:**
1. Make sure the repo is public (the updater downloads from the release without a token).
2. Generate the key: `cd app && npx tauri signer generate -w ~/.tauri/delilah.key`,
   and paste the public key into `plugins.updater.pubkey` in `tauri.conf.json`
   (until then it holds `REPLACE_WITH_UPDATER_PUBKEY` and `check.mjs` refuses to release).
3. Store the key's password in the Keychain for the dry run:
   `security add-generic-password -s delilah-updater -a "$USER" -w` (it prompts for it).
4. As `FranPerezFolgado`: `gh secret set TAURI_SIGNING_PRIVATE_KEY < ~/.tauri/delilah.key`
   and `gh secret set TAURI_SIGNING_PRIVATE_KEY_PASSWORD`.
5. **Back up the key file and its password** somewhere outside this Mac. An app only
   accepts updates signed with the key whose public half it shipped with: if the key
   is lost, no installed Delilah can ever update again and each Mac needs a manual
   reinstall with a new key.

**Bootstrap:** the updater only exists from the first release with it. Install that
release (0.8.0) by hand, once per Mac: download the `.dmg` from the GitHub release,
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
never overwrites; keeps the 3 newest; a failure only logs a warning). A downgrade also
makes a copy, named after the version in the marker. To go back:
1. Stop the daemon (Delilah → "Detener el daemon") **and** quit the app (⌘Q). Do not
   reopen it until the copy is in place.
2. Move `delilah.db` (and any `-wal`/`-shm`) aside.
3. Copy the backup to `$DELILAH_HOME/delilah.db`.
4. Delete `$DELILAH_HOME/version` and move the backup you used out of `backups/`. A
   version from before auto-update does not write the marker, and an existing
   `delilah-<v>.db` is never overwritten, so without this the next upgrade would reuse
   a stale marker or skip its copy; this way it makes a fresh one.
5. Install the older version and open it.

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
- **`.claude/launch.json`:** only `preview` (slot 8) for the desktop app's preview tools; nothing that binds 7420.
- **`.claude/settings.json`:** a `PreToolUse` hook (`.claude/hooks/guard-installed-daemon.sh`) that blocks agent
  commands against the installed app: killing or unloading `delilahd`, `scripts/dev-daemon.sh`, writes to the 7420 API,
  the CLI without `DELILAH_URL`, deleting `~/.delilah`. Run those yourself when you really mean it.
- **`.claude/verify.sh`:** the checks the `implementer` agent's stop hook runs (see Checks).
