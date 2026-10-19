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
```

With the fake fixture, start the daemon with `DELILAH_ALLOW_FILE_PROTOCOL=1`
(its submodules use `file://`).

## Design

The design sketch (decisions, architecture, MVP scope) lives in the
[Delilah design doc](https://claude.ai/code/artifact/f0306b42-531c-43d7-a2b3-79f94f160204).
