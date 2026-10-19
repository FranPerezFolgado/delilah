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

# A Delilah.app bundle in app/src-tauri/target/release/bundle/
cd app && npm run tauri build
```

`npm run dev` alone serves the same UI at http://localhost:1420 for a browser.

## Design

The design sketch (decisions, architecture, MVP scope) lives in the
[Delilah design doc](https://claude.ai/code/artifact/f0306b42-531c-43d7-a2b3-79f94f160204).
