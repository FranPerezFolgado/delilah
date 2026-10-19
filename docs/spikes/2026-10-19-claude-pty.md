# Spike: Claude Code in a PTY (2026-10-19)

Claude Code 2.1.224, run by `crates/delilah-agents/examples/claude_pty_spike.rs`
inside a workspace of the fake parent repo.

## What works

| Need | How | Result |
|---|---|---|
| Hooks without touching the repo | `claude --settings <file>` with the hooks in a file outside the worktree | `SessionStart`, `UserPromptSubmit`, `Stop` and `SessionEnd` arrive with the session id |
| Know the session id up front | `claude --session-id <uuid>` | Delilah picks the id; no need to parse it from hooks |
| Resume | `claude --resume <uuid>` (also with `-p`) | Claude remembers the previous turn |
| Usage windows | `statusLine` command in the same settings file; Claude pipes its JSON on every turn | `rate_limits.five_hour` / `seven_day` with `used_percentage` and `resets_at` (epoch seconds) |
| Folder trust | A new folder shows the trust dialog first | Accepted by sending Enter to the PTY; Claude remembers it per path |

Example status line payload:

```json
{"five_hour": {"resets_at": 1792422000, "used_percentage": 49},
 "seven_day": {"resets_at": 1792508400, "used_percentage": 51}}
```

## Gotchas

- **Clean environment.** Launched from inside another Claude Code session, the child
  inherits `CLAUDE_CODE_CHILD_SESSION` and friends and does not save its transcript,
  so `--resume` fails with "No conversation found". The daemon must spawn agents
  with a clean environment (launchd gives it one).
- **Trust dialog per worktree.** Every new workspace path shows it once. Answering
  in the PTY works; pre-trusting would mean writing to the global `~/.claude.json`,
  which we avoid.
- **`rate_limits` only for subscribers**, and only after the first API response of
  the session.

## Limit detection (implemented, real limit not observed yet)

The limit could not be triggered on demand. `delilah-agents::limits` implements
this rule; confirm it against the logged hooks the first time a real limit hits:

1. Keep the latest `rate_limits` per session from the status line.
2. A turn that ends (`Stop`, or `StopFailure`, which Claude emits instead after API
   errors) while `five_hour` or `seven_day` is at 100 % means "paused by limit",
   resuming at the latest `resets_at` among the full windows. A `StopFailure` whose
   payload mentions a limit also pauses, until the nearly full window resets (or
   for 30 minutes if none is known).
3. Log every raw hook payload so the real `StopFailure` shape can be checked the
   first time it happens on the work Mac.
