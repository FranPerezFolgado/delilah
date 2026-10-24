# Chat view and experimental chat mode: notes before the feature

Context gathered before launching the feature (not a spike result yet). Goal:
show task sessions as a chat like Claude Desktop, and optionally run a task in
an experimental "chat mode" on `stream-json` instead of the PTY.

## Where we are

- Every agent is an interactive Claude in a PTY owned by the daemon
  (`delilah-agents::pty::PtySession`), shown with xterm.js (`TerminalPane`).
- State comes from hooks only (decision #8): `UserPromptSubmit`, `PostToolUse`,
  `Notification`, `Stop`, plus `PreToolUse` for the ask
  (`crates/delilah-agents/src/claude.rs`, `classify` → `Signal`).
- The idea conversation already has a chat view over the PTY (decision #36):
  `conversation.rs` (`conversation_entries`, cards, queued messages, "Detener")
  and `app/src/components/ConversationView.tsx`. It shows only the user's
  messages, Claude's closing message (from `Stop`) and cards: no intermediate
  text, no tool calls.
- #36 left `stream-json` "for later" because it replaces the PTY.
- `docs/status.md` already lists "streaming of Claude's text and a chat view for
  other sessions" as pending.

## Three levels considered

1. **Chat from the transcript (recommended first).** Claude Code writes every
   message to `~/.claude/projects/<cwd>/<session>.jsonl` (assistant text,
   `tool_use`, `tool_result`, thinking). Hooks carry `transcript_path`, and the
   clean env (decision #10) is what makes the transcript exist. The daemon tails
   it, maps lines to entries and announces a topic (decision #11); the UI renders
   bubbles and tool cards (Bash, Edit with diff, Read…, collapsible).
   - Keeps PTY, `--resume`, hooks, `delilah attach` and the terminal fallback.
   - Works for every session, not just idea conversations.
   - Per message, not per token. The JSONL format is not a public contract:
     parse tolerantly, ignore unknown types/fields.
2. **More hooks** (`PreToolUse`/`PostToolUse` for all tools): cheap, gives tool
   input/output but not Claude's intermediate text. Superseded by 1.
3. **Experimental chat mode on `stream-json`** (what Claude Desktop / the Agent
   SDK do): `claude -p --input-format stream-json --output-format stream-json`
   (plus `--include-partial-messages` for token deltas). Streaming text and, with
   `--permission-prompt-tool`, approvals as UI cards. Replaces the PTY for that
   task, so it must be opt-in per task.

## Chat mode as an opt-in per task

**Expected to stay the same** (to confirm in the spike):
- Hooks with `--settings` should still fire in print mode, so `Signal`, phases,
  inbox and usage-limit detection keep working unchanged.
- `--resume <session>` works in both modes over the same session, so a task could
  switch mode between turns (escape hatch if chat mode misbehaves).
- Worktrees, PR, handoff, review flow.

**Needs work:**
- **Runtime abstraction.** `PtySession` is used concretely across the daemon
  (`run.rs`, `handoff.rs`, `conversation.rs`, `actions.rs`, `terminal.rs`). Needs
  something like `enum Runtime { Pty(PtySession), Stream(StreamSession) }` with
  send prompt (keystrokes vs a JSON line on stdin), stop (ESC vs interrupt /
  kill), wait/exit. Biggest and riskiest change.
- **Event parser** for stream-json (`system`, `assistant`, `user` with
  `tool_result`, `stream_event` deltas, `result`). Final messages become entries
  (same shape as level 1); deltas go live over the WebSocket only, never stored.
- **Permissions.** No TUI to approve in. Option A: keep the current allow list +
  `dontAsk` + ask hook (cheap). Option B: `--permission-prompt-tool` backed by a
  Delilah MCP tool, giving "allow?" cards (nicer, adds an MCP server).
- **Folder-trust dialog**: the PTY flow accepts it by reading the screen (the one
  exception to #8); check whether print mode skips it.
- **Data/UI:** a per-task flag (migration, never edit a shipped one), a toggle in
  `NewTaskDialog`, `TaskScreen` shows the chat instead of `TerminalPane`;
  `delilah attach` and the terminal are disabled for chat-mode sessions (CLI and
  API must say so clearly).
- **Contracts:** new endpoints/topics in `api.rs` ↔ `app/src/api.ts`.

**Risks:**
- Two runtimes to maintain and test.
- The stream-json *input* protocol is thinly documented and may change between
  Claude versions.
- A stuck chat-mode task has no terminal to look at; switching to PTY between
  turns mitigates it.

## Suggested order

1. **Spike (half a day):** run `claude -p` with stream-json in/out and Delilah's
   `--settings`; confirm (a) hooks reach the daemon, (b) the process takes several
   turns over stdin, (c) `--resume` works across PTY ↔ stream, (d) what happens
   with folder trust and with a tool not in the allow list, (e) what an interrupt
   looks like. If (a)–(c) fail, the cost of chat mode goes up a lot.
2. **Level 1:** transcript-fed chat view for task and project sessions, reusing
   and extending `ConversationView` (message bubbles + tool cards). Terminal stays
   one click away.
3. **Chat mode (experimental flag):** runtime abstraction + stream parser feeding
   the same entries, plus live deltas and permission handling.

Steps 2 and 3 can be one feature with phases or two features; 3 depends on 2's
view components.

Decisions to record in `docs/decisions.md` when taken: transcript as a data
source (amends #8: still hooks for state, transcript only for display), chat
mode as a second runtime (amends #36), the permission option chosen.
