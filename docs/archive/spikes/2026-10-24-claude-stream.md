# Spike: Claude Code in print mode with stream-json (2026-10-24)

Claude Code 2.1.292, run by `crates/delilah-agents/examples/claude_stream_spike.rs`
in fixture workspaces, with the settings file shaped like `claude::settings` (all hook
events posting to a local listener, plus the status line) and a clean environment
(`env -i` plus `agent_env`). Command:

```
claude -p --input-format stream-json --output-format stream-json --verbose \
  --include-partial-messages --session-id <uuid> --settings <file> [--permission-prompt-tool stdio]
```

Answers research.md R7 (a-h). Verdict: **adjust**, see decision 67 in
[decisions.md](../../decisions.md).

## R7 answers

| # | Question | Result | Evidence |
|---|---|---|---|
| a | Hooks fire in print mode | **Yes** | `SessionStart`, `UserPromptSubmit`, `PreToolUse`, `PostToolUse`, `Stop`, `SessionEnd` all arrive (turns: 1 start, 3 prompts, 3 stops, 1 end). `Notification` and `StopFailure` are registered but never fired (nothing triggered them): unverified, not disproved. |
| b | One process, several turns, transcript, stream matches transcript | **Yes** | `turns`: 3 prompts in one process, 3 `result`s, transcript written (45 lines). Stream `assistant`/`user` messages carry the transcript line's `uuid` and `message.id` exactly (permission run: 8 stream messages, same 8 uuids in the transcript). The stream does **not** echo the user's own prompt, only `tool_result` user messages. Each `assistant` message is one content block; blocks of one API message (thinking then text) are separate messages sharing `message.id`, as in the transcript. |
| c | `--resume` PTY -> stream -> PTY | **Yes** | `resume`: the stream leg answered "alpha", from the PTY turn; transcript 40 -> 54 -> 69 lines on one session id. |
| d1 | Print mode does not wait on the trust dialog in a new folder | **Yes** | `trust`: brand-new git folder and plain folder, both ran straight to a `result`, no dialog. The Write there was refused (`system/permission_denied`, "Claude requested permissions to write to ..., but you haven't granted it yet") and `hello.txt` was **not** written in either folder: print mode skips the dialog, but does not grant anything. |
| d2 | `can_use_tool` control requests | **Yes, with a different flag** | Needs `--permission-prompt-tool stdio`. R7 assumed `--permission-prompts host` as the default: that flag is not what produces them. Without the flag Claude denies by itself (`system/permission_denied`, `result.permission_denials`) and no `control_request` is sent. Shapes below. |
| d3 | AskUserQuestion through the blocking ask hook | **Yes, but only with the stdio flag** | Without `--permission-prompt-tool stdio`, AskUserQuestion is not in `system/init` `tools` in `-p` (`resume`: false). With it: present, the PreToolUse ask hook answered, the tool result was `Your questions have been answered: "Which colour do you prefer?"="Red"`, and Claude said it was red. No `control_request` was sent for it: the hook answers first. Also works in the PTY (`ask-pty`). Two PreToolUse posts at the same millisecond are by design: the settings have a `*` matcher (generic event post) and an `AskUserQuestion` matcher (the ask endpoint). |
| e | Interrupt | **Yes** | `interrupt`: `{"type":"control_request","request_id":"x","request":{"subtype":"interrupt"}}` got `control_response` `{"subtype":"success","request_id":"x","response":{"still_queued":[]}}`, then a `user` message `[Request interrupted by user]` and a `result` with `subtype: error_during_execution`, `is_error: true`, `terminal_reason: aborted_streaming`. The process stayed up; the next user line ran a new turn (`init`, `assistant` "after-interrupt", `result` success). Hooks: `SessionStart`, `UserPromptSubmit` x2, one `Stop` (after the second turn), `SessionEnd`. **No `Stop` for the interrupted turn**, as with ESC in the PTY (decision 36). |
| f | Status line runs in print mode | **No** | No `statusline.jsonl` exists in any stream-only scenario. The settings are right (`statusLine` as `claude::settings` writes it), and the same settings in the PTY legs (`resume`, `ask-pty`) ran it 10 times each with `rate_limits` (`five_hour` 46 %, `seven_day` 29 %). The spike summary line "status line runs: 0" is wrong for the PTY legs (an example bug, not looked into); the `statusline.jsonl` files are the evidence. Alternative: the stream's `rate_limit_event`, below. |
| g | Deltas with a message id | **Yes, through `message_start`** | `stream_event` wrappers have `session_id`, `uuid` (unique per event, **not** a message id) and `parent_tool_use_id`. The id comes from the `message_start` event (`event.message.id`, e.g. `msg_011CgMSkwt3...`); the following `content_block_delta`s (`text_delta`, `thinking_delta`, `input_json_delta`, `signature_delta`) belong to it until `message_stop`. The complete `assistant` message has the same `message.id`. |
| h | `--remote-control <name>` with `-p` | **Accepted, unverified** | The process started, answered and exited 0; stderr empty; `init` has no remote-control field. Nothing in the files shows a session registered on claude.ai. |

## Shapes

`can_use_tool` (stdout), for `npm --version` not in any allow list:

```json
{"type":"control_request","request_id":"2cb8c269-...",
 "request":{"subtype":"can_use_tool","tool_name":"Bash","tool_use_id":"toolu_01WLN...",
  "input":{"command":"npm --version","description":"Show npm version"},
  "display_name":"Bash","description":"Show npm version",
  "decision_reason":"This command requires approval","decision_reason_type":"other",
  "permission_suggestions":[{"type":"addRules","behavior":"allow","destination":"localSettings",
    "rules":[{"toolName":"Bash","ruleContent":"npm --version"}]}]}}
```

Answers (stdin) that worked:

```json
{"type":"control_response","response":{"subtype":"success","request_id":"2cb8c269-...",
 "response":{"behavior":"allow","updatedInput":{"command":"npm --version","description":"Show npm version"}}}}
{"type":"control_response","response":{"subtype":"success","request_id":"0b44928c-...",
 "response":{"behavior":"deny","message":"Denied by the spike"}}}
```

The deny reached Claude: "It was denied with the message 'Denied by the spike'", and
`result.permission_denials` listed the call. The suggestion is a rule per command
(`npm --version`), not per tool, as R8 assumed.

Without the stdio flag, a refused tool gives (stdout):

```json
{"type":"system","subtype":"permission_denied","tool_name":"Bash","tool_use_id":"toolu_...",
 "decision_reason":"This command requires approval","message":"This command requires approval"}
```

`rate_limit_event` (stdout, once per turn, after the first API response):

```json
{"type":"rate_limit_event","session_id":"...","uuid":"...",
 "rate_limit_info":{"status":"allowed","rateLimitType":"five_hour","resetsAt":1792872000,
  "isUsingOverage":false,"overageStatus":"rejected","overageDisabledReason":"org_level_disabled",
  "unifiedWindows":{"five_hour":{"utilization":0.41,"resetsAt":1792872000},
                    "seven_day":{"utilization":0.29,"resetsAt":1793113200}}}}
```

Same data as the status line's `rate_limits` (`used_percentage` 41-46 there, `utilization`
0.41 here: a fraction, not a percent; same `resets_at` epoch seconds). `status` is
`allowed` here; the value when a limit is hit has not been observed.

Other stdout types seen: `system/init` (once per turn, with `tools`, `mcp_servers`,
`permissionMode`), `system/status`, `system/hook_started`/`hook_response` (SessionStart
hooks), `system/thinking_tokens`, `result` (`subtype`, `is_error`, `terminal_reason`,
`permission_denials`).

## Gotchas

- **`--permission-prompt-tool stdio` is the switch.** It makes permission requests
  arrive as `control_request` and also adds AskUserQuestion to the tools of a `-p`
  session. Without it, chat mode cannot ask the user anything.
- **Without the flag `-p` refuses, it does not ask.** Writes in a folder without allow
  rules are denied, in new folders too.
- **No `Stop` after an interrupt.** The turn ends with a `result` (`error_during_execution`);
  the daemon must end the turn from it.
- **The user's prompt is not echoed** on stdout; the transcript has it (with its own
  `uuid`).
- **No status line in print mode.** Limits must come from `rate_limit_event`.
- **Trust dialog (2.1.292) defaults to "No, exit"** in the PTY: the daemon's
  `accept_trust_dialog` already handles it, and the examples now do the same.
- **Clean env still loads the user's claude.ai MCP connectors.** An "Eden connector
  needs authorization" note appeared in a result. Not a Delilah problem, but it shows in
  answers.
- **`TodoWrite` is absent** from the `-p` init tools.
- **`DONT_ASK_ALLOW`** in `crates/delilah-agents/src/claude.rs` lists AskUserQuestion;
  in `-p` it exists only with the stdio flag.
- Run 1's `resume` and `ask` scenarios were a broken example and were re-run (spike2).
