# Research: Agent playground

## R1. Where the current action and the helpers come from

- **Decision**: Claude Code hooks, as for the session state. The settings Delilah passes
  with `--settings` (`delilah-agents/src/claude.rs`, `HOOK_EVENTS`) add `PreToolUse`
  (matcher `*`, next to the existing `AskUserQuestion` entry), `PostToolUseFailure`,
  `SubagentStart` and `SubagentStop`, all posting to the existing `/hooks/claude`.
  - `PreToolUse` → the action starts (`tool_use_id`, `tool_name`, `tool_input`).
  - `PostToolUse` / `PostToolUseFailure` with the same `tool_use_id` → it ends.
  - `SubagentStart` / `SubagentStop` (`agent_id`, `agent_type`) → a helper starts or ends.
  - Any event that carries `agent_id` belongs to that helper, not to the main agent. It
    was confirmed in this task's own `hooks.jsonl`: a `code-researcher` subagent's
    `PostToolUse` carried `agent_id` and `agent_type`.
- **Rationale**: it is the same pipeline as `LiveState` (decision "state from hooks"),
  and it has no lag. It is also what pixel-agents and claude-office use.
- **Alternatives**:
  - Tailing the transcript JSONL under `~/.claude/projects`, as pixel-agents does as a
    fallback. It is a second source of truth and a file format that is not a contract.
  - Parsing the PTY screen. It is fragile.

## R2. Kinds of action

- **Decision**: map `tool_name` to a kind in the adapter, with a pure function:

  | Kind | Tools |
  |---|---|
  | `edit` | Edit, MultiEdit, Write, NotebookEdit |
  | `read` | Read, Grep, Glob, LS, WebFetch, WebSearch, NotebookRead |
  | `run` | Bash, BashOutput, KillShell |
  | `delegate` | Agent, Task |
  | `other` | everything else (MCP tools `mcp__*`, TodoWrite, AskUserQuestion…) |

  The target comes from `tool_input`:
  - the file name without its directory (`file_path`, `notebook_path`);
  - `pattern` for Grep and Glob;
  - the start of `command` for Bash;
  - `description` (or `subagent_type`) for Agent;
  - the host of `url` for WebFetch;
  - `query` for WebSearch;
  - nothing otherwise.

  It is cut to 40 characters with "…" **in the daemon**, so nothing longer ever leaves
  it (FR-009, privacy).
- **Rationale**: the adapter is the only place that knows Claude Code's tool names, so a
  future Codex adapter maps its own. Cutting at the source bounds what the UI and the
  WebSocket ever see.
- **Alternatives**: sending the raw `tool_input` to the UI. That leaks whole commands and
  file contents (Write's `content`).

## R3. "Supervising"

- **Decision**: derived. The main agent is supervising when its current action is a
  `delegate` whose `run_in_background` is not true.
- **Rationale**: the hooks have no "blocked on a subagent" flag (checked in the hooks
  docs). A foreground Agent call is blocking by definition, and its `PostToolUse` comes
  when the subagent returns. A background call's Pre/Post pair is instant, so the main
  character goes back to its own work.
- **Alternatives**: counting running helpers. It is wrong for background helpers, while
  the main agent keeps working.

## R4. Lifetime and clearing

- **Decision**:
  - The action and the helpers live in memory, on the session's `Live`, and are never
    stored.
  - They are cleared when the session's state becomes `Waiting` or `Paused`, and on
    `Stop`, `StopFailure` and `SessionEnd`.
  - A helper is upserted by **any** event carrying an unknown `agent_id`; `agent_type`
    gives its role. So a background helper that outlives its parent's turn comes back
    with its next tool call after a `Stop` cleared it.
  - Helpers launched by helpers come with their own `agent_id`, so they are flat helpers
    of the same session.
  - One action per agent: a new `PreToolUse` replaces the current one. A
    `Post…` only clears it if its `tool_use_id` matches. That covers parallel tool
    calls well enough: the last one started is shown.
- **Rationale**: it meets FR-010 and FR-018. It heals itself if a `SubagentStop` or a
  `PostToolUse` never arrives, which covers crashes and the edge cases in the spec.
- **Alternatives**: timeouts, which are arbitrary and still wrong for long builds.

## R5. How the UI gets it

- **Decision**:
  - A new endpoint `GET /api/v1/activity`, which returns the action and helpers of every
    live Claude session.
  - A new WebSocket topic `activity`, emitted when either one changes.
  - `SessionView` is left unchanged.
- **Rationale**:
  - Actions change several times per second per agent. Folding them into `sessions`
    would make every listener of that topic (sidebar, task screen) refetch session
    records on each tool call.
  - The activity endpoint reads only memory.
  - The topic system already merges bursts, with a 60 ms debounce in `live.ts`.
- **Alternatives**: pushing payloads over the WebSocket. Today the socket only carries
  topic names, and changing that is a bigger contract change for no gain.

## R6. Rendering: DOM + CSS with pixel sprites drawn in code

- **Decision**:
  - Each character and desk is a DOM element (`<button>` for characters), drawn as an
    inline SVG from **pixel maps written as text** (`playground/sprites.ts`: rows of
    characters, one per pixel, mapped to palette tokens). It uses
    `shape-rendering: crispEdges`.
  - Animation frames are separate maps. A CSS `steps()` animation switches them.
  - The colours are CSS custom properties, so light and dark themes come for free.
  - `@media (prefers-reduced-motion: reduce)` freezes the animations to the first frame,
    with no fades and no pulse.
  - The office gets a `paused` class (`animation-play-state: paused`) while
    `document.hidden` is true or the connection is lost.
- **Rationale**:
  - **Accessibility** (FR-007, FR-022): characters are real focusable buttons with
    `aria-label`. A canvas would need a parallel accessibility tree.
  - **Licensing** (FR-013): the art is original, written in the repo and reviewable in a
    diff, with no binary assets.
  - **Cost**: no new dependency.
  - **Performance**: 16 small SVGs with CSS animations are composited by the browser,
    with no JavaScript loop (SC-004).
- **Alternatives**:
  - Canvas 2D, as pixel-agents does. It needs a JavaScript loop, a hand-made
    accessibility layer and theme handling in code.
  - PixiJS or Phaser. Ruled out in the spec.
  - Kenney CC0 sprite sheets. There are not enough office and role sprites, and the
    style would be mixed.

## R7. Desk layout

- **Decision**:
  - One `<section>` per project, labelled with its name, in the order of the projects
    list. Project-owned sessions (`@project`) sit in their project's area, labelled
    with the project instead of a task.
  - Inside each area, a CSS grid of fixed slots. Slots are assigned by a pure function
    `assignSlots(previous, sessions)`: kept sessions keep their slot, a new one takes the
    lowest free slot, and an ended one frees its slot.
  - The mapping lives in a ref, so state changes never move a desk (FR-021).
- **Rationale**: it is testable without a DOM, and it is stable across refetches.
- **Alternatives**: sorting by start time, where desks shift when one in the middle ends.

## R8. The separate window

- **Decision**:
  - **Desktop app**: the UI creates a Tauri `WebviewWindow` labelled `playground` with
    URL `index.html#playground`. If one exists, it is focused instead. The capability
    file adds that window and `core:webview:allow-create-webview-window`.
  - **Window content**: with that hash, `main.tsx` renders `PlaygroundWindow`, which is
    just the office, with no sidebar.
  - **Click on a character there**: `emitTo("main", "delilah:open-task", { task,
    session })`, then `main` is shown, unminimised and focused. The main window is never
    destroyed: the shell turns its close into a hide (`main.rs`). `App` listens for the
    event and calls its existing `openTask(key, sessionId)`.
  - **Browser preview**: `window.open(url + "#playground", "delilah-playground")`. The
    named target reuses the tab. Clicks go through a `BroadcastChannel("delilah")`, and
    the main tab handles them the same way.
  - Both are wrapped in `platform.ts` (`openPlaygroundWindow`, `onOpenTask`,
    `requestOpenTask`), which already wraps the other desktop-only calls.
- **Rationale**: there are no custom Rust commands; the shell keeps owning only native
  things, as decided before. The webview talks to the daemon directly, like the main
  window.
- **Alternatives**:
  - A Rust command that builds the window. It needs more shell code for no gain.
  - Routing the click through the daemon. That would put UI state in the daemon.

## R9. Data the view needs

- **Decision**: one hook, `usePlayground()`, used by both the view and the window:
  - sessions, tasks, specs (for the phase via `columnOf`, the same rule as the board and
    the header in feature 003), projects and activity;
  - refetched on the `sessions`, `tasks`, `specs`, `projects` and `activity` topics;
  - plus `online` from `connect()`.
- **Rationale**: the separate window has no `App`, so the view must load its own data.
  Duplicate fetches in the main window are cheap.
- **Alternatives**: passing the data down from `App`. The separate window would then
  need a second loader anyway.

## R10. Agents launched before the change

- **Decision**: no migration. Settings are written when an agent is launched or resumed.
  Old agents just send no `PreToolUse` or `Subagent*` events, so they have no action
  (thinking pose) and no helpers, as the spec's edge case says.
