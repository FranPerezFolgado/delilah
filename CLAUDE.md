# Delilah

Native macOS orchestrator for coding agents, written in Rust. Design doc:
https://claude.ai/code/artifact/f0306b42-531c-43d7-a2b3-79f94f160204
UI mock-ups: https://claude.ai/artifact/6kgvh73UPJWaboZVJiddre

## Current scope: MVP-0

Core daemon + CLI, plus a first desktop app (Tauri, `app/`). Only the Claude Code
adapter. The tasks are listed
in the design doc's "MVP" section.

## Ground rules

- Developed on a personal Mac, tested on the work Mac. Never reference or copy
  anything from the work repos; use `scripts/make-fixture-parent.sh` and Jira
  JSON fixtures instead.
- Agent hooks go in a settings file outside the repo, passed with `claude --settings`;
  never in the worktree or the global `~/.claude`. Spawn agents with a clean env.
- Git and gh here run as the personal account (FranPerezFolgado). The machine
  also has work credentials: confirm the account before any remote operation.
- Submodules over `file://` need `git -c protocol.file.allow=always`.

## Trying changes: always in a preview

Delilah is built with Delilah: the installed app (daemon on 7420, `~/.delilah`)
runs the agents working on this repo, so never restart, stop or `pkill` any
`delilahd` to try a change, and never use 7420 or `scripts/dev-daemon.sh` for it.
Try changes in a preview of the branch instead:

```bash
scripts/preview.sh            # this worktree's daemon + UI, in the first free slot
scripts/preview.sh --fresh    # from an empty state
```

It prints the UI URL (http://localhost:150N) and the API (http://127.0.0.1:750N),
with its own state in `fixtures/preview-home-N` and the fixture repos to register
as projects. Open the UI in a browser (or the browser pane) to try the feature;
use `DELILAH_URL=<API> target/debug/delilah …` or curl for the API. Run it in the
background and stop it with Ctrl-C (or by stopping that process), never by name.
Only what is native to macOS (notifications, menu, Dock badge, global ⌘P) needs
the Tauri app; say so instead of trying it.
