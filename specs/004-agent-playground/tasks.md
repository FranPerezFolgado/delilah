# Tasks: Agent playground

**Input**: Design documents from `specs/004-agent-playground/`
**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/api.md, quickstart.md

**Tests**: test-first. Write each test task before its implementation and see it fail.

**Checks**:
- Adapter: `cargo test -p delilah-agents`
- Daemon: `cargo test -p delilah-daemon`
- UI: `cd app && npx tsc --noEmit && npx vitest related --run <files>`
- At the end: `cargo test --workspace`, `cargo test --manifest-path app/src-tauri/Cargo.toml` and `cd app && npx vitest run`

**Never** try anything on port 7420 or with `pkill`: use `scripts/preview.sh` (see quickstart.md).

## Format: `[ID] [P?] [Story] Description`

## Phase 1: Setup

**Purpose**: the folder for the view and its art credits. There are no new dependencies.

- [ ] T001 Create `app/src/playground/CREDITS.md`. It says that every sprite in `sprites.ts` is original pixel art drawn for Delilah, under the repository's licence, and has a table (sprite, author, licence, source) with one row per sprite group, filled as the sprites are added (FR-013).
- [ ] T002 [P] Add the playground strings to `app/src/i18n/en.ts`, following the file's Spanish-key → English pattern:
  - "Playground" (nav and title);
  - "{n} esperando" → "{n} waiting";
  - "Abrir en ventana" → "Open in window";
  - "Ningún agente trabajando" → "No agent is working";
  - "Reconectando…" → "Reconnecting…";
  - "Supervisando" → "Supervising";
  - "Pensando" → "Thinking";
  - "Arrancando" → "Starting";
  - "Esperándote" → "Waiting for you";
  - "En pausa" → "Paused";
  - "y {n} más" → "and {n} more".

---

## Phase 2: Foundational (blocks all stories)

**Purpose**: the Playground view exists in the sidebar and loads the data it needs, before any visuals.

- [ ] T003 Write failing tests in `app/src/playground/layout.test.ts` for:
  - `assignSlots(prev: Map<string, number>, ids: string[]): Map<string, number>`:
    - empty prev gives 0..n-1 in input order;
    - kept ids keep their slot when others are added or removed;
    - a new id takes the lowest free slot (a hole left by an ended one);
    - the result never moves an existing id.
  - `groupByProject(sessions, tasks, projects, specs)`:
    - only `live` sessions of kind `claude`; shell and non-live sessions are dropped;
    - areas in the order of `projects`, and only projects with a desk;
    - a task session gets the key, the title and the phase via `columnOf(task, specs[task.key])` from `app/src/phases.ts`;
    - an `@name` session is grouped under project `name` and labelled with the project, with no phase;
    - a session whose task has `project: null` goes in a trailing "—" area.
- [ ] T004 Implement `assignSlots` and `groupByProject` (with the `DeskModel` and `ProjectArea` types) in `app/src/playground/layout.ts`, until T003 passes.
- [ ] T005 Implement `app/src/playground/usePlayground.ts`. It loads `api.sessions()`, `api.tasks()`, `api.projects()` and the specs map (the same call `App.tsx` uses for the board) in parallel, and refetches each one on its `on(topic)` from `app/src/live.ts`. It tracks `online` through the connection status, using `connect()` only if no connection exists in this window; otherwise it follows the existing status mechanism, and you should read `live.ts` to pick the right one. It returns `{ areas, online }`, built with `groupByProject`.
- [ ] T006 Add `"playground"` to the `View` union and a nav button ("Playground", the same pattern as Board and Inbox) in `app/src/components/Sidebar.tsx`. Add a test case in `app/src/components/Sidebar.test.tsx`: clicking the button calls `onView("playground")`.
- [ ] T007 Add `case "playground"`, rendering a lazy `PlaygroundView`, to the view switch in `app/src/App.tsx`, passing `onOpen={(task, session) => openTask(task, session)}`. Create a placeholder `app/src/playground/PlaygroundView.tsx` that renders the areas and desks as plain text, so the wiring can be tried.

**Checkpoint**: `npx vitest related --run src/playground/layout.ts src/components/Sidebar.tsx` passes; in a preview, Playground lists the live sessions grouped by project.

---

## Phase 3: User Story 1 — See every working agent at a glance (P1) 🎯 MVP

**Goal**: one desk and character per live agent, labelled with task and phase, with a look per state, updating live; an empty office when nothing runs.

**Independent Test**: start two tasks in a preview and open Playground. There are two desks with key, title and phase, and the characters change as the agents start, work and stop.

- [ ] T008 [P] [US1] Write failing tests in `app/src/playground/layout.test.ts` for `deskState(session, activity?)`:
  - `starting` → `starting`; `waiting` → `waiting`; `paused` → `paused`;
  - `working` with no activity → `thinking` (the other kinds come in US3).
- [ ] T009 [P] [US1] Write failing tests for `Sprite` in `app/src/playground/Sprite.test.tsx` (jsdom):
  - a 2×2 map with `.` and two palette letters renders an `<svg>` with `shape-rendering="crispEdges"` and one `<rect>` per horizontal run of one colour;
  - `.` renders nothing;
  - fills are `var(--pg-<key>)`;
  - with `frames=2` the frames sit side by side and the root carries `--pg-frames: 2`.
- [ ] T010 [US1] Implement `deskState` in `app/src/playground/layout.ts` (T008).
- [ ] T011 [US1] Implement `app/src/playground/Sprite.tsx` (T009).
- [ ] T012 [US1] Draw the first sprites in `app/src/playground/sprites.ts`:
  - **Format**: one string per row and one character per pixel; `.` is transparent, and each letter is a palette key (e.g. `k` outline, `s` skin, `h` hair, `c` clothes, `d` desk, `m` monitor, `l` screen light).
  - **Character**: 16×16, poses `arriving`, `thinking`, `waiting` (hand up and bubble) and `paused` (asleep with "z"), 2 frames each.
  - **Desk**: 32×16, with a monitor.
  - Export `POSES: Record<Pose, string[][]>` and `DESK`, and add a row per group to `CREDITS.md`.
- [ ] T013 [US1] Write failing tests in `app/src/playground/PlaygroundView.test.tsx` (jsdom; the pattern is `Sidebar.test.tsx`). Use a mocked `usePlayground` (`vi.mock("./usePlayground")`):
  - two areas render as `<section>`s with the project names as headings;
  - each desk shows the task key, the title (truncated, full text in `title`) and the phase label;
  - the character is a `<button>` with `data-pose` equal to `deskState`;
  - no desks → "Ningún agente trabajando";
  - re-rendering with one session removed and one added keeps the other desk's `data-slot`.
- [ ] T014 [US1] Implement `app/src/playground/Desk.tsx` (the desk, the main character `<button data-pose data-slot>` and the label) and `app/src/playground/PlaygroundView.tsx`:
  - project `<section>`s, each with a grid of slots from `assignSlots`, kept in a `useRef` per project;
  - the empty office;
  - a header with the title.

  This replaces the T007 placeholder, and T013 passes.
- [ ] T015 [US1] Create `app/src/playground/playground.css`:
  - the office background and the project areas;
  - the slot grid (`repeat(auto-fill, minmax(…))`, which wraps and scrolls past 8 desks);
  - the desk and label styles, with ellipsis on the title;
  - `--pg-*` palette variables built on the app's tokens, with dark values under the app's dark-theme selector (read `styles.css` to find it);
  - the `steps(var(--pg-frames))` frame animation.

  Import it from `PlaygroundView.tsx`.

**Checkpoint**: in a preview, the desks appear and disappear as agents start and end, and the poses follow the states.

---

## Phase 4: User Story 2 — Notice who needs me (P1)

**Goal**: waiting agents stand out, a counter shows how many there are, and a click opens the task on its terminal.

**Independent Test**: make one of two agents ask for permission. Its desk stands out, the counter reads 1, and clicking its character opens that task on its terminal.

- [ ] T016 [P] [US2] Write failing tests in `app/src/playground/layout.test.ts` for `accessibleName(desk)`:
  - `"LIBRE-3, Esperándote"` for a waiting desk;
  - `"LIBRE-3, Pensando"` for working with no action;
  - an `@proj` desk uses the project name.

  (The action and helper parts come in US3/US4.)
- [ ] T017 [US2] Implement `accessibleName` in `app/src/playground/layout.ts`, using `t()` for the state words (T016).
- [ ] T018 [US2] Add failing cases to `app/src/playground/PlaygroundView.test.tsx`:
  - a waiting desk has class `desk--waiting`, and the header counter reads "1 esperando";
  - the count includes desks in every area;
  - clicking a character, or focusing it and pressing Enter, calls `onOpen(task, session)` with the session's owner and id;
  - each character's `aria-label` is `accessibleName`.
- [ ] T019 [US2] Implement the waiting highlight, the counter and the click/keyboard handlers in `app/src/playground/Desk.tsx` and `PlaygroundView.tsx`. In `playground.css`, `.desk--waiting` gets an accent outline, a pulsing glow and the bubble pose, so the cue is not colour alone. T018 passes.

**Checkpoint**: in a preview, a permission prompt highlights the desk, and the click lands on the agent's terminal.

---

## Phase 5: Separate window (Clarification Q1; serves US1 and US2)

**Goal**: "Abrir en ventana" opens the office in a window of its own, and a click there brings the main window to the task.

**Independent Test**:
- In a preview, the button opens a tab with only the office, and pressing it again reuses that tab.
- A click in that tab makes the main tab open the task.
- The native window is checked in the desktop app; see the quickstart.

- [ ] T020 [P] [US2] Write failing tests in `app/src/platform.test.ts` (jsdom, browser mode, so `isDesktop` is false):
  - `openPlaygroundWindow()` calls `window.open` with a URL ending in `#playground` and the target `"delilah-playground"`;
  - `requestOpenTask({task, session})` posts `{type: "open-task", task, session}` on a `BroadcastChannel("delilah")`;
  - `onOpenTask(cb)` calls `cb` for that message and returns an unsubscribe.

  Stub `BroadcastChannel` with a tiny in-memory fake.
- [ ] T021 [US2] Implement `openPlaygroundWindow`, `requestOpenTask` and `onOpenTask` in `app/src/platform.ts`:
  - **Browser**: as in T020.
  - **Desktop**, through `@tauri-apps/api`:
    - `WebviewWindow.getByLabel("playground")` then `setFocus()`, or else `new WebviewWindow("playground", { url: "index.html#playground", title: "Delilah — Playground", width: 1100, height: 700 })`;
    - `emitTo("main", "delilah:open-task", p)`, then `unminimize()`, `show()` and `setFocus()` on the `main` window;
    - `listen("delilah:open-task")` for `onOpenTask`.

  Use dynamic imports, as the file already does for the other Tauri APIs. T020 passes.
- [ ] T022 [US2] Update `app/src-tauri/capabilities/default.json`:
  - `"windows": ["main", "playground"]`;
  - add `core:webview:allow-create-webview-window`, `core:window:allow-set-focus`, `core:window:allow-show` and `core:window:allow-unminimize` (and `core:event:default` if it is not already part of `core:default`).

  Update the description string. Run `cargo test --manifest-path app/src-tauri/Cargo.toml`, which also builds and so validates the capability.
- [ ] T023 [US2] Create `app/src/playground/PlaygroundWindow.tsx`: the office full-window, with a slim top bar (title and counter, no "open in window" button), and clicks going to `requestOpenTask`. In `app/src/main.tsx`, render it instead of `<App/>` when `location.hash === "#playground"` (keep the existing providers and styles).
- [ ] T024 [US2] Add the "Abrir en ventana" button to the `PlaygroundView.tsx` header, calling `openPlaygroundWindow()`. In `app/src/App.tsx`, register `onOpenTask(({task, session}) => openTask(task, session))` once in an effect. Add a test case in `PlaygroundView.test.tsx`: the button calls the mocked `openPlaygroundWindow`.

**Checkpoint**: in a preview, the separate tab works both ways.

---

## Phase 6: User Story 3 — See what each agent is doing right now (P2)

**Goal**: the daemon knows each agent's current action, and the character shows its kind with a caption.

**Independent Test**: watch an agent read a file, edit it and run the tests. Its character goes reading → typing → terminal, and the caption follows.

### Adapter

- [ ] T025 [P] [US3] Write failing tests in `crates/delilah-agents/src/claude.rs` (`#[cfg(test)]`):
  - **`settings()`**:
    - `hooks` contains `PostToolUseFailure`, `SubagentStart` and `SubagentStop` posting to `/hooks/claude`;
    - `PreToolUse` has two entries, the `AskUserQuestion` one to `/hooks/claude/ask` (unchanged) and a `"*"` one to `/hooks/claude` with no 86400 timeout.
  - **`action_of(tool, input, id)`**, table-driven:
    - Edit, MultiEdit, Write and NotebookEdit → `edit`, with the file name and no directory;
    - Read, Grep, Glob, LS, WebFetch, WebSearch and NotebookRead → `read`, where Grep and Glob give `pattern`, WebFetch gives the host and WebSearch gives `query`;
    - Bash, BashOutput and KillShell → `run`, with the start of `command`;
    - Agent and Task → `delegate`, with `description`, and `background` from `run_in_background`;
    - `mcp__jira__search` and `TodoWrite` → `other`, with no target;
    - a 100-character command → a target of 40 characters ending in "…" (char-safe with a multibyte string);
    - Write's `content` never appears.
  - **`classify_activity`**:
    - `PreToolUse` without `agent_id` → `ToolStarted { agent: None, .. }`;
    - with `agent_id` and `agent_type` → `agent: Some(AgentRef)`;
    - `PostToolUse` and `PostToolUseFailure` → `ToolEnded`;
    - `SubagentStart` → `HelperStarted`; `SubagentStop` → `HelperEnded`;
    - `Stop`, `StopFailure` and `SessionEnd` without `agent_id` → `TurnOver`;
    - `Notification` → `None`.
  - **`classify()`** still returns `Other` for `PreToolUse` and for `Subagent*`.
- [ ] T026 [US3] Implement the T025 changes in `crates/delilah-agents/src/claude.rs`:
  - add the events to `HOOK_EVENTS`;
  - build `PreToolUse` with both entries;
  - add `ActionKind`, `Action`, `AgentRef`, `ActivityEvent`, `action_of` and `classify_activity`, all deriving `Serialize` with kebab-case kinds.

### Daemon

- [ ] T027 [P] [US3] Write failing unit tests in `crates/delilah-daemon/src/activity.rs` for `Activity::apply(event) -> bool`:
  - **Main agent**:
    - `ToolStarted` sets the action (true);
    - `ToolEnded` with the same id clears it (true), and with another id leaves it (false);
    - a second `ToolStarted` replaces the first;
    - `TurnOver` clears the action and the helpers.
  - **Helpers**:
    - `ToolStarted` for an unknown helper upserts it with its role and action;
    - `HelperStarted` twice gives one helper;
    - `HelperEnded` removes it, and for an unknown id returns false;
    - helpers keep their start order;
    - `clear()` returns whether anything was there.
- [ ] T028 [US3] Implement `crates/delilah-daemon/src/activity.rs` (`Activity`, `Helper`, `apply` and `clear`, plus the serde view matching contracts/api.md) and register the module in `lib.rs`/`main.rs` (T027).
- [ ] T029 [US3] Write failing router tests in `crates/delilah-daemon/src/hooks.rs` (`mod tests`; reuse the AppState and `post(state, path, body)` helpers) against a live Claude session. Insert it with the same helper the existing state tests use; read them first:
  1. post a `PreToolUse` Edit, and `GET /api/v1/activity` shows the action with `kind: "edit"` and the file name;
  2. post the matching `PostToolUse`, and the session is gone from the response;
  3. post a `PreToolUse`, then a `Notification`, and the activity is cleared and the state is `waiting`;
  4. a `PreToolUse` for an unknown session is ignored (200, nothing in activity);
  5. a `Topic::Activity` event is emitted on a change and not on a no-op. Use the events subscription the existing tests use, if there is one; otherwise assert through a receiver from `AppState`.
- [ ] T030 [US3] Implement the daemon side until T029 passes:
  - `Topic::Activity` in `crates/delilah-daemon/src/events.rs`, with its wire name `activity`;
  - `Live.activity: Arc<Mutex<Activity>>`, plus `update_activity(event)`, which emits on change, in `crates/delilah-daemon/src/sessions.rs`;
  - `set_state` clearing the activity, and emitting, on `Waiting` or `Paused`;
  - a `classify_activity` call in `claude_hook` in `crates/delilah-daemon/src/hooks.rs`;
  - `GET /api/v1/activity` in `crates/delilah-daemon/src/session_api.rs` and its route in `crates/delilah-daemon/src/api.rs`.

### UI

- [ ] T031 [P] [US3] Add `ActionKind`, `Action`, `Helper`, `Activity` and `api.activity()` to `app/src/api.ts`, matching contracts/api.md, and `"activity"` to `Topic` in `app/src/live.ts`.
- [ ] T032 [US3] Extend the tests in `app/src/playground/layout.test.ts`:
  - **`deskState`** with an activity:
    - `edit`, `read`, `run` and `other` map to their poses;
    - `delegate` with `background: false` → `supervising`;
    - `delegate` with `background: true` → `thinking`;
    - a `waiting` session ignores any activity.
  - **`caption(action)`**: `"Edit · api.rs"`, `"Bash · cargo test…"`, and only the tool name with no target.
  - **`accessibleName`** includes the caption when there is one.
- [ ] T033 [US3] Implement the T032 changes in `app/src/playground/layout.ts`. Make `usePlayground.ts` also load `api.activity()` on the `activity` topic and attach each session's activity to its `DeskModel`.
- [ ] T034 [US3] Draw the action poses in `app/src/playground/sprites.ts` and add them to `CREDITS.md`:
  - `typing` (hands on keys, screen flicker);
  - `reading` (open book or paper);
  - `terminal` (green screen lines);
  - `phone` (supervising);
  - `busy` (other).

  2 frames each, in the same style as T012.
- [ ] T035 [US3] Show the caption under the character in `app/src/playground/Desk.tsx`; it is cleared when there is no action. Add cases to `PlaygroundView.test.tsx`: a desk with an `edit` action has `data-pose="typing"` and the caption "Edit · api.rs", and a foreground delegate shows `phone` and "Supervisando".

**Checkpoint**: `cargo test -p delilah-agents -p delilah-daemon` passes. In a preview, a newly started agent's character follows its tools.

---

## Phase 7: User Story 4 — See each agent's helpers around its desk (P3)

**Goal**: up to 4 small helpers in a semicircle around the desk, with a look per role and their own action. Any beyond 4 go in a "+N" badge. Helpers fade in and out.

**Independent Test**: have an agent launch two helpers of different roles in parallel. Two small characters with different looks appear around its desk, animate by their own actions, and fade out as they end.

- [ ] T036 [P] [US4] Write failing tests in `app/src/playground/layout.test.ts`:
  - **`roleLook(role)`**:
    - `code-researcher`, `Explore` and `doc-searcher` → `researcher`;
    - `implementer` → `implementer`;
    - `code-quality-reviewer` and `spec-compliance-reviewer` → `reviewer`;
    - `foo` → `{ look: "generic", label: "foo" }`.
  - **`visibleHelpers(helpers)`**: 6 helpers → the first 4 by `since`, plus `rest` with the other 2.
  - **`accessibleName`** appends the helpers ("2 ayudantes: code-researcher leyendo, implementer editando").
- [ ] T037 [US4] Implement `roleLook`, `visibleHelpers` and the `accessibleName` extension in `app/src/playground/layout.ts` (T036).
- [ ] T038 [US4] Draw the role props in `app/src/playground/sprites.ts` (magnifying glass, hard hat, glasses, generic badge), each a small overlay map on the 16×16 grid, and add them to `CREDITS.md`.
- [ ] T039 [US4] Add failing cases to `app/src/playground/PlaygroundView.test.tsx`:
  - a desk with 2 helpers renders 2 `.helper` elements, each with `data-look` and `data-pose` from its own action, at semicircle spots 0 and 1;
  - with 6 helpers, 4 render and a "+2" badge has `title` listing the other two roles;
  - when a helper is removed, the remaining ones keep their `data-spot`.
- [ ] T040 [US4] Render the helpers in `app/src/playground/Desk.tsx`:
  - each one is a non-interactive `<span class="helper" role="img" aria-label=…>` at 60 % scale, with the pose and the role prop;
  - spots are kept stable with a small `assignSlots` per desk;
  - an exit keeps a removed helper for 300 ms with `.helper--leaving`.

  In `playground.css`, add the semicircle positions and the fade in and out. T039 passes.

**Checkpoint**: in a preview, asking an agent to use two subagents in parallel shows them around its desk.

---

## Phase 8: User Story 5 — A view worth a screenshot (P4)

**Goal**: coherent pixel art, light and dark themes, reduced motion, no animation while hidden or offline.

**Independent Test**: with four running agents, screenshots in light and dark both read clearly, with no clipped labels or overlap. With reduce motion on, the characters hold still.

- [ ] T041 [P] [US5] Add failing cases to `app/src/playground/PlaygroundView.test.tsx`:
  - with `online: false` the office has class `office--stale` and shows "Reconectando…";
  - when `document.hidden` is true (stub `visibilityState` and fire `visibilitychange`), the office has class `paused`, and the class goes away when the document is visible again.
- [ ] T042 [US5] Implement the stale banner and the visibility pause in `app/src/playground/PlaygroundView.tsx`, and use the same component in `PlaygroundWindow.tsx`. T041 passes.
- [ ] T043 [US5] Finish `app/src/playground/playground.css`:
  - `.office--stale` (dimmed, `animation-play-state: paused` on all descendants) and `.paused`;
  - `@media (prefers-reduced-motion: reduce)`: `animation: none` (first frame), no transitions, no pulse, and helpers that appear and go without fading;
  - dark-theme palette values;
  - an office floor and a wall strip per project area, using CSS gradients in the pixel style;
  - labels that never overflow at a width of 1024 px.
- [ ] T044 [US5] Polish the art in a preview (`scripts/preview.sh`) with 4 agents. Take screenshots in light and dark, at reduced scale, using the `ui-verifier` agent so the images stay out of the main context. Fix contrast, clipping and overlap in `sprites.ts` and `playground.css`.

---

## Phase 9: Polish & Cross-Cutting

- [ ] T045 [P] `README.md`: a Playground section covering:
  - what each pose means;
  - the helpers and their roles;
  - the waiting counter and click-to-answer;
  - "Abrir en ventana";
  - that agents started before the update show no actions until relaunched.
- [ ] T046 [P] `docs/status.md`: the playground is built; known limits (only the last of parallel tool calls is shown; agents launched before the update). `docs/architecture.md`: the activity module, the `activity` topic and endpoint, the new hook events, and `app/src/playground/`.
- [ ] T047 [P] `docs/decisions.md`:
  - the playground is DOM + SVG pixel maps (canvas, game engines and third-party sprite sheets ruled out, and why);
  - activity is in memory on its own topic;
  - "supervising" is derived from a foreground `Agent` call.
- [ ] T048 Run every check (`cargo test --workspace`, `cargo test --manifest-path app/src-tauri/Cargo.toml`, `cd app && npx tsc --noEmit && npx vitest run`), then run quickstart.md end to end in a preview and note anything that only the desktop app can show (the native window).

---

## Dependencies & Execution Order

- **Setup** (T001–T002) → **Foundational** (T003–T007) → stories.
- **US1** (T008–T015) needs Foundational.
- **US2** (T016–T019) needs US1's `Desk`/`PlaygroundView`.
- **Separate window** (T020–T024) needs US2's click handling.
- **US3**:
  - the daemon tasks (T025–T030) depend on nothing in the UI and **can start right after Setup, in parallel with Foundational, US1 and US2**;
  - the UI tasks (T031–T035) need US1 and T030's contract.
- **US4** (T036–T040) needs US3; its daemon side is already done in T026–T030.
- **US5** (T041–T044) needs US1; best done after US4 so the art pass covers everything.
- **Polish** (T045–T048) comes last.

Within each story, a test task comes before its implementation and must fail first.

## Parallel opportunities

- T001 ∥ T002.
- **Daemon track** (T025 → T026 → T027 → T028 → T029 → T030) ∥ **UI track** (T003 … T024). They share no files.
- T008 ∥ T009; T016 ∥ T020 (different files); T031 ∥ T032; T045 ∥ T046 ∥ T047.

## Implementation strategy

1. **MVP**: Setup + Foundational + US1 + US2. With these, the office shows states and the waiting agents, and lets you jump to them. It is useful on its own.
2. **Increments**:
   - add the separate window;
   - add US3 (the daemon track, usually already done in parallel, plus the UI poses);
   - add US4 helpers;
   - add the US5 art and motion pass.
3. **Try each checkpoint** in a preview, never on 7420.
