# Delilah

Local orchestrator for coding agents (Claude Code, Codex, Pi…). It takes tasks
from Jira, runs them in isolated workspaces of a parent repo with submodules,
pauses and resumes on its own when an agent hits its usage limit, and notifies
you when it needs you.

Status: MVP-0 in progress (core daemon + CLI, Claude Code only).

## Development

The work repos are not available on this machine, so development runs against
a generated fake parent repo:

```bash
scripts/make-fixture-parent.sh
```

It creates `fixtures/parent-env/` (git-ignored) with a `parent` checkout whose
four modules (`svc-payments`, `svc-orders`, `svc-billing`, `common-lib`) are
submodules backed by local bare repos.

## Usage (MVP-0)

```bash
# Terminal 1: the daemon (state in ~/.delilah, API on 127.0.0.1:7420)
cargo run -p delilah-daemon

# Terminal 2: register projects once
delilah project add ~/work/plataforma --kind parent   # parent repo with submodules
delilah project add ~/dev/projects/signal --kind repo # a single repo, Orca-style

# Run a task: builds the workspace and starts Claude Code on it
delilah run PROJ-1234 --project plataforma --submodules svc-payments,svc-orders --title "Card retries"
delilah run SIGNAL-12 --project signal --title "Better feed"

delilah ls                 # tasks and their state
delilah attach PROJ-1234   # live terminal of the agent; Ctrl-] detaches
delilah usage              # subscription usage windows, as Claude reports them
delilah resume PROJ-1234   # resume a task paused by the usage limit right away
delilah done PROJ-1234     # mark it done and stop its agent
delilah archive PROJ-1234  # remove its workspace (refuses while work is unpushed)
```

A task whose turn ends on a usage limit is paused until the window resets; the
daemon then resumes it on its own and tells the agent to carry on. The daemon
sends macOS notifications when a task needs you, pauses, resumes or ends
(`DELILAH_NOTIFY=0` turns them off).

With the fake fixture, start the daemon with `DELILAH_ALLOW_FILE_PROTOCOL=1`
(its submodules use `file://`).

## Desktop app

A Tauri 2 shell around a React UI (`app/`). The UI only talks to the daemon's HTTP
and WebSocket API, so the shell could be swapped for Electron without touching it.

```bash
# 1. The daemon: against the fake repos (fixtures/, created on first run)…
scripts/dev-daemon.sh
#    …or against your real projects
cargo run -p delilah-daemon

# 2. The app (first time: npm install)
cd app && npm run tauri dev

# Delilah.app and a .dmg in app/src-tauri/target/release/bundle/ (builds delilahd too)
cd app && npm run dist
```

`npm run dev` alone serves the same UI at http://localhost:1420 for a browser.

The bundle carries `delilahd` inside (`Delilah.app/Contents/MacOS/delilahd`). When
the installed app opens, it installs a LaunchAgent
(`~/Library/LaunchAgents/dev.delilah.daemon.plist`) and launchd keeps the daemon
running from then on: at login, after a crash, and with the window closed, so
agents keep working. Opening a newer app restarts the daemon with the new binary.
The Delilah menu has "Reiniciar el daemon" and "Detener el daemon"; the log is
`~/Library/Logs/Delilah/delilahd.log`. Development builds (`tauri dev`) leave
launchd alone and expect `scripts/dev-daemon.sh`.

**Board and specs.** The Tasks view is a board with one column per SpecKit phase:
Especificación, Plan, Implementación, Revisión and Hecha. The daemon reads the
task's spec folder in its workspace every few seconds and works out the phase from
the files: no `plan.md` yet means Especificación, `plan.md` means Plan, unchecked
items in `tasks.md` mean Implementación, all of them checked means Revisión. The
folder is `specs/<folder>/` in the workspace (or in one of its submodules), the one
whose name has the ticket key or the branch name, or failing that the one the
branch has been writing to (a `specs/` kept in `.gitignore` counts too). Inside a task you see the phases, the spec's documents
and the `tasks.md` checklist with the item the agent is on. Phase changes go to
the task's history. Delilah only reads these files; the repo stays the source of
truth.

**Changes.** "Ver cambios" in a task shows what its branch changed in each repo of
the workspace (each submodule of a parent): the files, with their line counts, and
the diff of each. It compares against where the branch started (its merge-base with
`origin/HEAD`, or `main`), so commits, uncommitted edits and new files all count.
Arrow keys or j/k move through the files. When every item of `tasks.md` is checked,
the task offers to review the changes or start a "Revisión" session with Claude.

**Editor.** "Editor" in a task (or "Abrir en el editor" in the changes view) opens
the workspace's file tree, with the branch's changes marked, beside an editor
(CodeMirror 6, language picked from the file name). ⌘S saves, ⌘W closes the tab, ⌘F
searches. The agent works on the same files, so an open file reloads by itself when
it changes on disk. If you had unsaved edits, the editor asks which version to keep
rather than overwrite either one. Files above 1 MB and binaries are not editable.
Paths are confined to the workspace, symlinks included.

## Installing on another Mac

Delilah is not signed, so the simplest way is to build it on the Mac that will use
it. Needs Xcode Command Line Tools (`xcode-select --install`), Rust (rustup), Node
22+, git and Claude Code logged in.

```bash
# The repo is private: clone it with a read-only deploy key or your personal
# account, not a work one.
git clone git@github.com:FranPerezFolgado/delilah.git && cd delilah/app
npm install
npm run dist   # tauri build, with a .dmg that needs no Finder automation
# Install it
cp -R src-tauri/target/release/bundle/macos/Delilah.app /Applications/
open /Applications/Delilah.app
```

The `.dmg` in `src-tauri/target/release/bundle/dmg/` also works on another Apple
Silicon Mac. Copied over (AirDrop, a download), macOS will say the app is damaged
or from an unidentified developer, because it is not signed or notarized. Once it
is in /Applications, clear the quarantine flag:

```bash
xattr -dr com.apple.quarantine /Applications/Delilah.app
```

State lives in `~/.delilah` (database, workspaces, task files) and the Jira token
in the Keychain. To uninstall: Delilah → Detener el daemon, then delete the app,
`~/Library/LaunchAgents/dev.delilah.daemon.plist` and, if you want, `~/.delilah`.

**Jira.** Connect it from the app's Jira view: Jira Cloud (email + API token),
Server/Data Center (personal access token) or built-in sample data. The token is
kept in the macOS Keychain. "Analizar con Claude" opens a new task prefilled from
the ticket; for parent projects, "Sugerir con Claude" runs a quick read-only
Claude pass (haiku) that picks the submodules and says why. The agent gets the
ticket as `~/.delilah/tasks/<KEY>/ticket.md`.

**Sessions.** A task's workspace can hold several sessions: its primary Claude
(whose turns drive the task's status), extra Claude sessions (e.g. "Revisar
cambios", to review the branch) and plain terminals with your login shell. A usage
limit hit by any of them pauses the task; after the reset the primary and the
paused ones carry on. A session that stopped (the daemon restarted, the process
ended) can be reopened: Claude picks its conversation back up with `--resume`.

## Design

The design sketch (decisions, architecture, MVP scope) lives in the
[Delilah design doc](https://claude.ai/code/artifact/f0306b42-531c-43d7-a2b3-79f94f160204).
