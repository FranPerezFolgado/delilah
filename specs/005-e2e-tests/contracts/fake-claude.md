# Contract: `fake-claude`

Binary `crates/delilah-e2e/src/bin/fake-claude.rs`. The daemon runs it through the
wrapper `<root>/bin/claude-fake`:

```sh
#!/bin/sh
exec '<target>/fake-claude' --fake-dir '<root>/fake' "$@"
```

## Arguments (the same as Claude Code's, from `claude::command`)

`--fake-dir <dir>` (wrapper only, always first), then any of:

| Arg | Use |
|---|---|
| `--session-id <id>` / `--resume <id>` | session id and `mode` (exactly one) |
| `--settings <path>` | **required**; hooks are read from it |
| `--add-dir <dir>` (repeated) | recorded |
| `--remote-control <name>` | recorded |
| `--permission-mode <mode>`, `--setting-sources <s>`, `--strict-mcp-config` | recorded |
| `--plugin-dir <dir>`, `--append-system-prompt <text>` | recorded |
| `-- <prompt>` | the prompt; searched for `[fake:<name>]` |

An unknown option is recorded, not an error: Claude Code adding a flag must not break
the fake, but the smoke test (R10) is where a new flag gets noticed.

A missing `--settings`, or both or neither of `--session-id`/`--resume`: prints the
error and exits 2.

## Hooks

For an event `E`, the fake takes `settings.hooks[E]`, keeps the entries whose
`matcher` is `*` (or, for `PreToolUse`, `AskUserQuestion`), and runs each
`hooks[].command` with `sh -c`, the payload JSON on stdin, `cwd` its own cwd, and
the hook's `timeout` (seconds; default 60). The hook's stdout is kept for
`PreToolUse`.

Payloads (fields the daemon reads, as in `hooks.rs` tests):

| Event | Payload |
|---|---|
| `SessionStart` | `{hook_event_name, session_id, cwd, source: "startup" \| "resume"}` |
| `UserPromptSubmit` | `{…, prompt}` |
| `PreToolUse` | `{…, tool_name: "AskUserQuestion", tool_input: {questions: [{question, header, multiSelect: false, options: [{label, description}]}]}}` |
| `PostToolUse` | `{…, tool_name, tool_input, tool_response}` |
| `Stop` | `{…, last_assistant_message, background_tasks?}` (only from the `stop` object form with `background`) |
| `StopFailure` | `{…, error}` |
| `Notification` | `{…, notification_type: "idle_prompt", message}` |
| `SessionEnd` | `{…, reason}` |

`PreToolUse` answer: the stdout is parsed as JSON and
`hookSpecificOutput.updatedInput.answers` is recorded (what `asks.rs` returns). Empty
stdout (the daemon released the ask) records `null` and the fake prints the question
to the terminal and waits for a line, as Claude would.

The status-line command is not run (FR-006b).

## Terminal

It writes plain text to stdout (the PTY). The folder-trust dialog, for
`claude::trust_dialog_selection` to recognise it:

```text
Do you trust the files in this folder?
<cwd>
❯ 1. Yes, proceed
  2. No, exit
```

The dialog comes before `SessionStart`: the hook only runs after it is accepted
(declined: only `SessionEnd`, exit 1).

Then it reads input: `\r` or `\n` → yes; `2` then Enter → no (exit 1).

## Transcript

At start it creates an empty `<HOME>/.claude/projects/<cwd with non-alphanumerics as
dashes>/<session id>.jsonl`, which is what the daemon checks before passing `--resume`.
It does so only if `HOME` is absolute and equals the Env's temp home (`<fake-dir>/../home`)
and the session id has no `/` or `..`; otherwise nothing is written.

## Scenario lookup

Order: `[fake:<name>]` in the prompt → `<task>.<command>.json` → `<task>.json` →
`default.json`.

`<command>` is the prompt's leading slash command without the slash, up to the first
space (`/feature LIBRE-1 …` → `feature`, `/orchestrate specs/x` → `orchestrate`). It must
pass `valid_name` once joined (`<task>.<command>`); a prompt without a leading `/word`
skips this lookup. So a task's plan and implement sessions can play different scripts.

## The `stop` step

- `{"stop": "text"}`: sends `Stop` with `last_assistant_message = text`, no
  `background_tasks`.
- Object form, for a turn that ends with background work still running:

```json
{"stop": {"message": "Waiting on the reviewer.",
          "background": [{"id": "a1", "type": "subagent", "agent_type": "architecture-reviewer",
                          "description": "Review plan", "status": "running"}]}}
```

  Sends `Stop` with `last_assistant_message = message` and `background_tasks =
  background` (each entry as given; `[]` allowed). `background` left out: no
  `background_tasks` field.

## The `write` step

```json
{"write": {"path": "specs/libre-1-x/tasks.md", "content": "- [ ] T001 …\n"}}
```

- `path` is relative to the fake's cwd (the task's workspace). Absolute paths, `..`
  components or an empty path: the fake prints the error and exits 2 (a broken scenario,
  like an unknown step).
- Parents are created; the file is overwritten.
- Sends no hook (the daemon reads the spec from disk at the next turn end).

## The `fail` step

Prints the message and runs `StopFailure`, then idles like after a stop (answers each
line typed; EOF -> `SessionEnd`, exit 0). Real Claude stays alive after a failed turn,
and the daemon's inbox error item lasts until `SessionEnd`.

## Guarantees

- No network apart from the hook commands it is given (loopback curl to the daemon).
- Never reads `$HOME/.claude`, never runs `claude`.
- Exits on EOF or SIGHUP on stdin (PTY closed), after `SessionEnd`.
