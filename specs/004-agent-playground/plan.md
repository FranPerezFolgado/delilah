# Implementation Plan: Agent playground

**Branch**: `feature/LIBRE-3` | **Date**: 2026-10-20 | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `specs/004-agent-playground/spec.md`

## Summary

A new **Playground** view shows the live Claude sessions as a small pixel-art office. It
has one area per project and one desk per session, with a character whose look shows the
session state, its current action and its helper subagents. The view can also be opened
in a window of its own.

- **Daemon**: the Claude adapter subscribes to `PreToolUse`, `PostToolUseFailure`,
  `SubagentStart` and `SubagentStop`. A new pure `classify_activity` turns those hooks
  into activity events. Each `Live` session keeps an in-memory `Activity` (current
  action plus helpers), served by `GET /api/v1/activity` and announced on a new
  `activity` topic.
- **UI**: the office is DOM and CSS. Sprites are written as text pixel maps and rendered
  as SVG, with theme colours from CSS variables, so the app gets no new dependency and
  no binary assets.
- **Separate window**: a Tauri `WebviewWindow` on `#playground` in the app, and a named
  tab in the browser preview.

See [research.md](research.md).

## Technical Context

**Language/Version**:
- Rust (2021 edition): `delilah-agents`, `delilah-daemon`, `app/src-tauri`
- TypeScript + React 19 (`app/`)

**Primary Dependencies**:
- Existing only: axum, serde, tokio (daemon); Vite, vitest, Testing Library (UI); Tauri
  v2 and `@tauri-apps/api` (window and events).
- No graphics library.

**Storage**: none (in-memory activity; no migration)

**Testing**:
- `cargo test -p delilah-agents` (classification, settings)
- `cargo test -p delilah-daemon` (activity transitions, `/api/v1/activity`, topic)
- `cd app && npx tsc --noEmit && npx vitest run` (slots, roles, captions, view)

**Target Platform**: macOS (Tauri app); the web UI in a preview for trying it

**Project Type**: desktop app (daemon + web UI + shell)

**Performance Goals**:
- Changes visible ≤ 2 s (SC-003); the WebSocket path is ~60 ms debounce plus one fetch.
- No JavaScript animation loop. Terminal echo is unaffected with 8 agents and 8 helpers
  (SC-004).

**Constraints**:
- Captions are capped at 40 characters, cut in the daemon.
- Graphics are original and recorded in `CREDITS.md`.
- Reduced motion is respected.
- Readable at the minimum window width (1024 px).

**Scale/Scope**: up to ~8 sessions × ~4 helpers on screen; 1 endpoint, 1 topic, 1 view,
1 window

## Constitution Check

`.specify/memory/constitution.md` is still the unfilled template, so the project's
ground rules (`CLAUDE.md`) serve as gates:

| Gate | Status |
|---|---|
| Nothing from the work repos; fixtures only | ✅ tests use JSON hook payloads written in the tests |
| Agent hooks in a settings file outside the repo, clean env | ✅ the new hooks go in the same `--settings` file; nothing in the worktree or `~/.claude` |
| Try changes only in a preview, never 7420 / pkill | ✅ quickstart uses `scripts/preview.sh` |
| No remote operations without confirming the account | ✅ none |
| Contracts (`api.rs` ↔ `api.ts`) | ⚠️ new endpoint and topic; additive, documented in [contracts/api.md](contracts/api.md); `SessionView` unchanged |
| Never edit a shipped migration | ✅ no migration |

Post-design re-check: no change. The only contract change is additive.

## Project Structure

### Documentation (this feature)

```text
specs/004-agent-playground/
├── spec.md
├── plan.md
├── research.md
├── data-model.md
├── quickstart.md
├── contracts/api.md
└── checklists/{requirements,ux}.md
```

### Source Code

```text
crates/delilah-agents/src/claude.rs      # HOOK_EVENTS += PostToolUseFailure, SubagentStart, SubagentStop;
                                         # PreToolUse "*" entry; classify_activity(), action_of(tool, input)
crates/delilah-daemon/src/activity.rs    # new: Activity, Action, Helper; apply(&mut Activity, ActivityEvent) -> bool;
                                         # clear(); serde views
crates/delilah-daemon/src/sessions.rs    # Live.activity; clear activity when state → Waiting/Paused
crates/delilah-daemon/src/hooks.rs       # claude_hook: classify_activity → live.activity apply → emit Topic::Activity
crates/delilah-daemon/src/events.rs      # Topic::Activity
crates/delilah-daemon/src/session_api.rs # GET /api/v1/activity (or a new activity_api.rs if it reads better)
crates/delilah-daemon/src/api.rs         # route
app/src/api.ts                           # Activity, Action, Helper, ActionKind; api.activity()
app/src/live.ts                          # Topic += "activity"
app/src/platform.ts                      # openPlaygroundWindow(), requestOpenTask(), onOpenTask()
app/src/playground/sprites.ts            # pixel maps (character poses × frames, desk, role props), palette keys
app/src/playground/Sprite.tsx            # map → SVG (crispEdges), frames via CSS steps()
app/src/playground/layout.ts             # assignSlots(), groupByProject(), roleLook(), caption(), deskState()
app/src/playground/layout.test.ts
app/src/playground/usePlayground.ts      # data hook: sessions, tasks, specs, projects, activity, online
app/src/playground/PlaygroundView.tsx    # office: header (waiting counter, "Abrir en ventana"), project areas, desks
app/src/playground/Desk.tsx              # desk + main character (button) + helpers semicircle + "+N"
app/src/playground/PlaygroundView.test.tsx
app/src/playground/playground.css        # office, poses, highlight, fades, reduced-motion, .paused
app/src/playground/CREDITS.md            # original art, author, licence
app/src/main.tsx                         # #playground → <PlaygroundWindow/>
app/src/components/Sidebar.tsx           # View += "playground"; nav button
app/src/App.tsx                          # case "playground"; onOpenTask listener → openTask()
app/src/i18n/en.ts                       # new strings
app/src-tauri/capabilities/default.json  # playground window + window/webview permissions
README.md, docs/status.md, docs/architecture.md, docs/decisions.md
```

**Structure Decision**: a new `app/src/playground/` folder, because the feature brings
its own components, art, CSS and helpers. Keeping them together leaves
`components/` uncluttered and puts the art next to its credits. The daemon gets an
`activity.rs` module next to `sessions.rs`, which keeps `hooks.rs` an adapter between
HTTP and state.

## Design

### 1. Adapter (`delilah-agents/src/claude.rs`)

- `settings()` adds the new events to `HOOK_EVENTS`, and builds `PreToolUse` as two
  entries: `AskUserQuestion` (unchanged) and `"*"` with `post_command`. A test asserts
  both entries are present and that the `"*"` entry has no long timeout.
- `action_of(tool_name, tool_input, tool_use_id) -> Action` uses the kind table and the
  target rules in research R2, and cuts the target to 40 characters with "…"
  (char-safe). Unit tests are table-driven: one case per tool, an unknown tool, an
  `mcp__` tool, a long command, a path with directories, and Write's `content` never
  appearing.
- `classify_activity(&Value) -> Option<ActivityEvent>`: `agent_id` and `agent_type`
  become `Some(AgentRef)` when present. Main-agent `Stop`, `StopFailure` and
  `SessionEnd` become `TurnOver`. Other events give `None`.

### 2. Daemon state (`activity.rs`, `sessions.rs`, `hooks.rs`)

- `Activity::apply(event) -> bool` follows the transition table in
  [data-model.md](data-model.md) and returns whether anything changed. It is pure and
  unit-tested, covering:
  - a mismatched `tool_use_id`;
  - an upsert from a tool event before `SubagentStart`;
  - a `SubagentStop` for an unknown helper;
  - the second of two parallel tools.
- `Live` gets `activity: Arc<Mutex<Activity>>` and `fn update_activity(event)`, which
  emits `Topic::Activity` when it changes. `set_state` to `Waiting` or `Paused` clears
  the activity, and emits if it was not empty.
- In `claude_hook`, after the existing signal handling, `classify_activity` runs and
  the result goes to `live.update_activity`. Only live Claude sessions have a `Live`, so
  hooks for unknown sessions are ignored, as they are today.
- `GET /api/v1/activity` returns the views of the live Claude sessions whose activity is
  not empty. Router tests use the existing `post(state, path, body)` helper:
  1. post a `PreToolUse`, then `GET` and see the action;
  2. post a `PostToolUse`, and the action is gone;
  3. post `SubagentStart` and a helper's `PreToolUse`, and the helper and its action are
     there;
  4. a `Notification` (→ Waiting) clears everything.

### 3. UI data (`api.ts`, `live.ts`, `usePlayground.ts`)

- The types and `api.activity()` mirror the contract.
- `usePlayground()` loads all five lists in parallel and refetches each one on its own
  topic. It also tracks `online` with `connect()`. In the main window it uses its own
  subscription; `on()` supports several listeners.
- It derives `desks: DeskModel[]` with `groupByProject()`:
  - one area per project that has a live Claude session, in the projects order;
  - project-owned (`@name`) sessions labelled with the project;
  - the phase from `columnOf(task, specs[task.key])`;
  - shell sessions and non-live sessions dropped.

### 4. Layout helpers (`layout.ts`)

- `assignSlots(prev, ids) -> Map<id, slot>` keeps existing slots and gives new ids the
  lowest free slot (FR-021).
- `roleLook(role)` follows the table in the data model.
- `deskState(session, activity)` returns `starting | thinking | edit | read | run |
  delegate | other | supervising | waiting | paused`.
- `visibleHelpers(helpers)` returns the first 4 by start time plus the rest, for the
  "+N" badge.
- `caption(action)` returns `"{tool} · {target}"`.
- `accessibleName(desk)` builds the FR-022 label.
- Each helper gets table-driven tests in `layout.test.ts`.

### 5. Rendering (`playground/*`)

**Sprites**
- Each sprite is a list of strings in a 16×16 grid for characters and 32×16 for desks.
- One character per pixel; `.` is transparent, and each letter maps to a palette key.
- Palette keys map to CSS variables in `playground.css`, built on the app's tokens, with
  dark values under the app's dark theme selector.
- `Sprite` renders `<svg viewBox shape-rendering="crispEdges">` with one `<rect>` per
  horizontal run of the same colour. Frames sit side by side, and the CSS
  `steps(n)` animation of `transform` on the inner group cycles them.

**Poses (main character)**

| Pose | Frames | Notes |
|---|---|---|
| arriving | 2 | |
| thinking | 2, slow | |
| typing | 2 | |
| reading | 2 | |
| terminal | 2 | |
| on the phone (supervising) | 2 | |
| busy (other) | 2 | |
| hand up with bubble (waiting) | 2 | |
| asleep with "z" (paused) | 2 | |

Helpers reuse the poses at 60 % scale, with a role prop layered on top.

**Desk**
- A `<button class="character">` with `aria-label={accessibleName}` and the task caption
  below it.
- Helpers are absolutely positioned on a semicircle behind the desk (4 fixed spots).
- `+N` is a `<span title=…>`.
- Waiting gets `.desk--waiting`: an accent outline and a pulsing glow, plus the bubble
  icon, so the cue is not colour only.
- Helpers fade in and out through a short CSS transition, with a keyed exit kept for
  300 ms.

**View**
- The header has the title, the waiting counter ("N esperando") and "Abrir en ventana".
- Below it, project `<section>`s, each a grid of desk slots.
- When there are no desks, an empty office with "Ningún agente trabajando".
- When offline, `.office--stale` (dimmed, `animation-play-state: paused`) and a
  "Reconectando…" banner.
- While `document.hidden`, `.paused` as well.

**Reduced motion**: `@media (prefers-reduced-motion: reduce)` sets `animation: none`
(first frame) and removes the transitions and the pulse.

**Clicks**: in the main window, `openTask(task, session)`; in the separate window,
`requestOpenTask`.

### 6. Separate window (`platform.ts`, `main.tsx`, `App.tsx`, capability)

- `openPlaygroundWindow()`:
  - **desktop**: `WebviewWindow.getByLabel("playground")`; if it exists, `setFocus()`.
    Otherwise `new WebviewWindow("playground", { url: "index.html#playground", title:
    "Delilah — Playground", width: 1100, height: 700 })`.
  - **browser**: `window.open(location.pathname + "#playground", "delilah-playground")`.
- `requestOpenTask(p)`:
  - **desktop**: `emitTo("main", "delilah:open-task", p)`, then the `main` window gets
    `unminimize()`, `show()` and `setFocus()`;
  - **browser**: a `BroadcastChannel` post, then `window.opener?.focus()`.
- `onOpenTask(cb)` subscribes to whichever applies. `App` registers it once and calls
  `openTask`.
- `main.tsx`: when `location.hash === "#playground"`, it renders `<PlaygroundWindow/>`
  (the view with its own top bar, no sidebar) instead of `<App/>`.
- The capability is updated as in the contract. The playground window has no menu
  actions of its own, so the shell is unchanged apart from config.

### 7. Docs

- `README.md`: a Playground section (what you see, waiting counter, own window).
- `docs/status.md`: the feature is built, plus the known limit that agents started
  before the update show no actions until restarted.
- `docs/architecture.md`: the activity module, the `activity` topic and the hook events.
- `docs/decisions.md`:
  - the playground is DOM + SVG pixel maps (no canvas, no engine, no sprite sheets);
  - activity is in memory and announced on its own topic;
  - "supervising" is derived from a foreground `Agent` call.

## Risks

- **Hook overhead**: a `curl` per tool call, twice. It goes to localhost with the
  existing `post_command` (short timeout, `|| true`), so it is a few milliseconds per
  call, the same as `PostToolUse` today.
- **Parallel tool calls**: only the last one started is shown. This is acceptable for a
  glance view.
- **Claude Code field drift**: `agent_id`, `agent_type` and `run_in_background` are
  read defensively, so a missing field means "main agent" or "foreground", never a panic.
  The raw payloads stay in `hooks.jsonl` for re-checking.
- **Art quality**: pixel maps are hand-written. Budget a review pass in the preview,
  with screenshots in light and dark (US5).

## Complexity Tracking

None. There is no new dependency, crate or storage.
