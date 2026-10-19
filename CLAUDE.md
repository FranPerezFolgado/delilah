# Delilah

Native macOS orchestrator for coding agents, written in Rust. Design doc:
https://claude.ai/code/artifact/f0306b42-531c-43d7-a2b3-79f94f160204
UI mock-ups: https://claude.ai/artifact/6kgvh73UPJWaboZVJiddre

## Current scope: MVP-0

Core daemon + CLI, no GUI. Only the Claude Code adapter. The tasks are listed
in the design doc's "MVP" section.

## Ground rules

- Developed on a personal Mac, tested on the work Mac. Never reference or copy
  anything from the work repos; use `scripts/make-fixture-parent.sh` and Jira
  JSON fixtures instead.
- Hooks for agents are installed per workspace (`.claude/settings.local.json`
  in the worktree, excluded from git), never in the global `~/.claude`.
- Git and gh here run as the personal account (FranPerezFolgado). The machine
  also has work credentials: confirm the account before any remote operation.
- Submodules over `file://` need `git -c protocol.file.allow=always`.
