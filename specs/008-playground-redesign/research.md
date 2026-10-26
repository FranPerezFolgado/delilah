# Research: Playground redesign

Sources: the code (daemon `activity.rs`, `sessions.rs`, `hooks.rs`, `transcript_watch.rs`,
`delilah-agents` `claude.rs` / `transcript.rs`; app `playground/*`, `TaskScreen.tsx`,
`platform.ts`), pixtuoid at commit `90320b132ef0f5e844685e711f0820652c354dca`
(2026-10-26, MIT, © 2026 Ivan Wang) and agent-flow (Apache-2.0, read only, nothing copied).

## R1. Activity history: where it lives

- **Decision**: a daemon-wide `ActivityLog` in `AppState`
  (`Arc<Mutex<ActivityLog>>`, new module `activity_log.rs`), not a field of `Live`. One
  `SessionLog { owner, events: VecDeque<Entry>, dropped: bool, last_at }` per session id,
  200 entries each. Every classified hook event of a Claude session is appended (even one
  that does not change `Activity`, e.g. the second of two parallel tool calls), with a
  daemon-wide increasing `seq` and its time. The log keeps at most 64 sessions: when a
  new one comes in past that, the one with the oldest `last_at` that is not live is dropped
  (live ones never are). That keeps ended sessions viewable (spec: up to ~50) without
  touching the session-end paths.
- **Rationale**: `Live` is dropped when a session ends, and its end has several paths
  (exit, archive, kill, restart); a log outside `Live` needs none of them. The hot path is
  one short mutex hold and a `VecDeque` push; hooks already take the `Live` mutexes.
- **Alternatives**: a `VecDeque` in `Live` (lost at session end; the spec wants ended
  sessions viewable); SQLite (decision #80 holds: seconds-long data, no persistence wanted);
  a lock-free queue (not needed at a few events per second).

## R2. Entries and the API

- **Decision**: `Entry { seq, at, agent: Option<AgentRef>, event }` where `event` is the
  serialised `ActivityEvent` (`tool_started` with the 40-char-cut `Action`, `tool_ended`
  with the tool use id and whether it failed, `helper_started`, `helper_ended`,
  `turn_over`). New endpoint `GET /api/v1/activity/history?owner=<owner>[&after=<seq>]`
  returns the logs of that owner's sessions (live and ended) with entries after `seq`. The
  `activity` topic is emitted on every append (not only on state changes), so open Flow
  views fetch `after=<last seq>`; the 60 ms client debounce already merges bursts.
- **Rationale**: per-owner is exactly what a floor/Flow shows; `after` keeps refetches
  tiny. Reusing the `activity` topic avoids a new topic for the same data.
- **Alternatives**: put the history in `GET /api/v1/activity` (every client of the topic
  would download every session's 200 entries on each event); a WebSocket payload with the
  event (the event bus carries only topics today, decision on refetch-on-topic).

## R3. Parallel tool calls and failure

- `PreToolUse` carries `tool_use_id`, so starts and ends pair by id; `PostToolUseFailure`
  becomes `ToolEnded` too — the entry records `failed: true` from the hook name so Flow can
  mark the node as an error. The office keeps showing one action per agent (unchanged).

## R4. Tokens per session

- **Decision**: extend the transcript watcher (`transcript_watch.rs`, 1 s tick + hook
  pokes): besides its newline check, it keeps per session a `TokenMeter { offset, total,
  input, output, cache_write, cache_read, last_message_id }` and parses the new complete
  lines from `offset` (partial last line left for the next tick). For each
  `type: "assistant"` line with `message.usage`, it skips the line if `message.id` equals
  the last one counted (Claude Code writes one line per content block, repeating the same
  usage), otherwise adds `input_tokens + cache_creation_input_tokens + output_tokens` to
  `total` (pixtuoid's "fresh spend"; `cache_read_input_tokens` kept separately, not in the
  total). On first sight the whole file is read once (in `spawn_blocking`). When `total`
  changes, `Topic::Activity` is emitted. `GET /api/v1/activity` gains
  `tokens: { <session id>: TokenUsage }` for live sessions.
- **Rationale**: the watcher already knows the trusted transcript path and runs every
  second; parsing only appended bytes is cheap. pixtuoid's tiers (250K / 2M / 16M) were
  tuned on fresh spend, so the same measure keeps the stack meaningful. pixtuoid does not
  dedupe by message id and over-counts; we do.
- **Limits**: helper (sub-agent) usage lives in separate files
  (`<session>/subagents/*.jsonl`) in current Claude Code and is not counted; spec
  Assumptions allow this. Only sessions with a readable transcript inside Claude's
  projects folder get a meter (same trust rule as today).
- **Alternatives**: hook payloads (carry no usage); chars/4 estimate like agent-flow (not
  real); counting cache reads (dominates and would put every session at the top tier).

## R5. Session states and `zzz`

- `LiveState` is Starting | Working | Waiting | Paused; a finished turn (`Stop`) is Waiting.
  There is no idle state, so `?` = Waiting, `zzz` = Paused (spec updated).

## R6. Porting pixtuoid's art

- **Format**: `.sprite` files: `#` comments, `@frame N`, `@mark name x y`, rows of
  space-separated one-char keys, `.` transparent; `pack.toml` has `[palette]` (~130 keys,
  hex), `[ramps]` (derived shades: `"h" = { of = "H", level = -2 }`, OKLab lightness
  steps) and `[animations.*]` (`frames`, `frame_ms`).
- **Decision**: a script `scripts/port-pixtuoid.py` (stdlib only, run by hand against a
  pinned pixtuoid checkout) writes one generated, checked-in file
  `app/src/playground/art/pixtuoid.ts`: the palette, the ramp table, each needed sprite
  as packed row strings (keys remapped to single ASCII/BMP chars where needed), marks, and
  animations with their frame timings. The pinned commit is in the file header and in
  `CREDITS.md`. Only the 1x art is ported (characters 8×10–8×12, desk 14×9): the `@4x`
  art is 16× the pixels and the hairstyles exist only at 4x, so hairstyles are left out
  (hair colour still varies via the `H` key).
- **Subset**: characters `seated`, `typing_0/1`, `standing`, `walking_0..7`,
  `walking_back_0..7`, `seated_sleeping`, `holding_coffee`; desk pieces `desk`,
  `desk_chair`, `desk_cup`, `token_tower` (3 frames); lounge `meeting_sofa`,
  `side_table`, `pantry_small`, `water_cooler`, `vending_machine`; decor `plant`,
  `plant_tall`, `whiteboard`, `bookshelf`, `wall_clock`; icons `icon_waiting`,
  `icon_idle` (used for `zzz` if it reads as sleep, otherwise our own 5×5 `z`).
- **Recolouring**: per character the base keys `B` shirt, `P` pants, `H` hair, `S` skin
  are set, and their ramp keys re-derived with a TS port of the OKLab step
  (`art/ramp.ts`, ~40 lines, unit-tested against values computed by the script for the
  default palette).
- **Outfits**: pixtuoid's 16 outfits (shirt/pants hex) and its seed
  (`splitmix64(fold(h*131+b))` over a string, BigInt in TS). Seed string = the project's
  name (same project → same outfit, FR-006). Hair (8 presets) and skin (5 presets) from
  the session id with the same hash, so two agents of one project still differ.
  Helpers: a fixed outfit per role (investigar/implementar/revisar/generic), keeping the
  role looks of spec 004.
- **Monitor glow**: pixtuoid's colours mapped to our kinds — edit `#64a0ff`, read
  `#50dcf0`, run `#f0aa50`, delegate `#c88cff`, other `#8cf0aa`; no action: the desk's
  idle glass. Drawn by recolouring the desk's glass keys (`j`/`J`) plus a soft halo
  rectangle, shared as `KIND_COLOURS` with Flow (FR-017).

## R7. Rendering (decision #79 kept)

- **Decision**: SVG pixel maps in the React tree, as today. A sprite frame is drawn as one
  `<path>` per colour (runs merged per row), with `shape-rendering: crispEdges` at an
  integer scale. Multi-frame animations render all frames and cycle them with a CSS
  `steps()` animation (no JS loop), so `animation-play-state` pauses them (hidden, stale)
  and reduced motion shows frame 0.
- **Walking**: no A*. A floor has a fixed geometry (pods of 2×2 desks on a grid, an aisle
  below them, a lounge corner on the right); a leaving helper's path is a 3-point polyline
  desk → aisle → lounge seat, animated with CSS transitions on `transform` per leg and
  the walking frames; then it fades out after ~5 s. Reduced motion: it appears on the
  sofa, then goes.
- **Alternatives**: canvas (decision #79: accessibility), porting pixtuoid's A*
  (unnecessary on a fixed floor), the `@4x` art (much heavier SVG).

## R8. Flow graph

- **Decision**: own deterministic layout, SVG, no new dependency. `flowModel.ts` (pure)
  turns the owner's entries into nodes (agent, helper, tool call), edges (agent→tool,
  agent→helper) and timeline rows; tool calls pair start/end by tool use id (an unpaired
  start is "running" while the session is live and "unknown end" after it ends). Helpers
  link to the lead agent (hook events do not say which helper launched a helper; spec
  edge case falls back to the lead). `flowLayout.ts` lays agents out as a tree left to
  right and each agent's tool calls in a column of rows beside it (newest at the bottom),
  so a node never moves once placed. Pan/zoom by dragging and wheel/pinch on an SVG
  `viewBox`, with +/−/fit buttons for keyboard users. The timeline is a list of buttons
  (keyboard reachable, FR-020); selecting a node scrolls/highlights its row and back.
- **Rationale**: ≤ 200 events per session and a handful of sessions per owner fits SVG;
  a stable layout reads better than d3-force jitter and needs no dependency.
- **Alternatives**: d3-force like agent-flow (non-deterministic, dependency), React Flow
  (heavy dependency), canvas (accessibility).

## R9. Where the views live in the app

- **Task screen**: a fixed tab `OFFICE` (`"\0office"`, kind `office`, label "Oficina")
  right after `OVERVIEW` in `TaskScreen.tsx`, for tasks and projects (owner `@name`). Its
  body is `OfficeTab` with the Oficina/Flujo toggle (`localStorage` key
  `delilah.office.view`, read in try/catch).
- **Building**: `PlaygroundView` renders `Building`. In the main window a floor click calls
  `openTask(owner, OFFICE)` (extend `openTask` to accept a tab id, today a session id). In
  the separate window (`PlaygroundWindow`) the floor opens in place (local state, Back
  button); a lead character click there still uses `requestOpenTask` to bring the main
  window forward on the terminal.
- **Data**: `usePlayground` becomes `useOffice()` (all owners: sessions, tasks, projects,
  activity, tokens) with `floorsByOwner`; `useActivityHistory(owner)` fetches the history
  and refetches `after` on the `activity` topic. Floors' desk slots stay stable in
  module-level maps keyed by owner (as `SPOTS`/`SLOTS` today); building floor order is
  first-seen order, newest on top.

## R10. Visual frame: the app's design system (user request)

Neither design artifact (the "Delilah" mock-ups canvas and the design doc) draws a
Playground, so the views follow the design system those mock-ups define (and UI v2
implements in `styles.css`):

- **Frame**: office, flow and each building floor sit in an inset box like the mock-ups'
  embedded "Consola" view: `border: 1px solid var(--bd); border-radius: 8px;
  background: var(--term); overflow: hidden`, with a 30px header row inside
  (`--panel`, border-bottom `--bd`, 12px text) holding the title/legend on the left and
  controls on the right. No gradients, no shadows (only overlays have one).
- **Toggle**: Oficina/Flujo is the mock-ups' segmented control (`role="tablist"`, 1px
  `--bd2`, radius 6, `.seg` 12px with a 13px line icon, `.seg.on` = `--sel` bg +
  `--tx`), right-aligned in the header row; the waiting count sits just before it as a
  `pill c-wait`.
- **Tab**: "Oficina" is a normal `.tab` in the existing strip (13px line icon, 12px label,
  active = `--panel` + `inset 0 2px 0 var(--acc)`), with a 6px `--acc` dot when an agent
  of the owner waits.
- **State colours come from the status tokens**, not from the art: waiting = `--acc`
  (the `?` bubble, the desk/floor highlight as an inset accent bar), paused = `--pause`
  (`zzz`), running = `--run`, error = `--err`, done = `--done`. The accent is used only for
  "needs you". The monitor glow per tool kind (pixtuoid colours) is the one exception: it
  is part of the art, inside the dark screen, and always named in the caption.
- **Flow nodes**: rectangles with radius 6, `--panel` fill, 1px `--bd2` border; state by
  border/badge colour from the status tokens (`--run` running, `--err` failed, `--done`
  done, `--tx3` unknown); tool kind as a small swatch of the glow colour plus its name.
  Labels 12px system font, targets in the mono font. Timeline rows 12px, times in mono
  11.5px, selected row = `--sel` + `inset 2px 0 0 var(--acc)`.
- **Building floors**: a list of framed rows (like the mock-ups' list rows): key in mono,
  title, phase with the phase icons, miniature strip on `--term`; waiting floor = left
  accent bar + `pill c-wait`.
- **Width**: the centre column can be ~600px with both side panels open; the floor scales
  down by integer steps (×5 → ×4 → ×3) to fit, and the flow graph fits on open.
- **Themes**: the pixel art keeps its own colours inside the `--term` inset in both themes;
  everything around it (frame, labels, bubbles, nodes) uses the tokens of `.dk` / `.lt`.
- **Token names in the app** (`app/src/styles.css`, UI v2) differ from the mock-ups: accent /
  "needs you" = `--wait` (`--wait-bg`, `--wait-tx`), paused = `--pause` (`--pause-bg`),
  running = `--run`, error = `--err`, done = `--ok`, terminal inset = `--term-bg`
  (`--term-tx`), plus `--panel`, `--bd`, `--bd2`, `--sel`, `--tx`/`--tx2`/`--tx3`. Use
  these wherever R10 above says `--acc`, `--done` or `--term`.
