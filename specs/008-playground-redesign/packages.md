# Work packages — 008-playground-redesign

Source: specs/008-playground-redesign/tasks.md · 32 pending tasks · 19 packages

Check names used below: `agents` = `cargo test -p delilah-agents`; `daemon` = `cargo test -p delilah-daemon`; `ui` = `cd app && npx tsc --noEmit && npx vitest related --run <files>` (add `--no-cache` after adding files under `app/src/i18n/en/`); `ui-all` = `cd app && npx tsc --noEmit && npx vitest run --no-cache`.

Scheduling notes:
- The daemon track (WP-02..WP-05) has no dependency on Phase 1 or the UI track; it starts in group 2A next to the art/client packages.
- Early start approved: WP-09 depends only on WP-06 and runs in group 2B (its phase stays 3); WP-12 runs alongside WP-11 in group 3C (its phase stays 4). Schedule by "Depends on".
- WP-14 and WP-15 only need WP-06 (types); they keep group 5A unless the orchestrator approves an early start.

## WP-01 Pixtuoid art port and credits
- Phase: 1 Setup
- Tasks: T001, T002, T003
- Files: scripts/port-pixtuoid.py, app/src/playground/art/pixtuoid.ts, app/src/playground/art/ramp.fixtures.ts, app/src/playground/CREDITS.md
- Depends on: —
- Parallel group: 1A
- Acceptance: the script parses pack.toml and the R6 sprite list (rejects ragged rows, remaps unsafe keys) and generates `pixtuoid.ts` (header with source URL, pinned commit 90320b13…, "do not edit"; PALETTE, RAMPS, SPRITES, ANIMATIONS) and `ramp.fixtures.ts`; generated from a clone in the scratchpad, never in the repo; CREDITS.md carries the pixtuoid MIT notice ("Copyright (c) 2026 Ivan Wang") and says agent-flow was reference only.
- Checks: ui
- Reviewers: security

## WP-02 Agents crate: failed tool calls and transcript token usage
- Phase: 2 Foundational
- Tasks: T004, T008
- Files: crates/delilah-agents/src/claude.rs, crates/delilah-agents/src/transcript.rs, crates/delilah-daemon/src/activity.rs
- Depends on: —
- Parallel group: 2A
- Acceptance: FR-014, FR-025 — `ActivityEvent::ToolEnded { failed }` is true for `PostToolUseFailure`; `usage_delta` counts one `message.id` once, leaves a partial last line unconsumed, skips non-assistant/bad JSON, treats missing numbers as 0 and excludes `cache_read` from `total`.
- Checks: agents, daemon
- Reviewers: contract

## WP-03 Activity log module
- Phase: 2 Foundational
- Tasks: T005
- Files: crates/delilah-daemon/src/activity_log.rs, crates/delilah-daemon/src/lib.rs
- Depends on: —
- Parallel group: 2A
- Acceptance: FR-013 — 200 entries per session (oldest dropped, `dropped` counted), 64 logs with eviction of the oldest non-live log (grows past 64 when all live), seq increasing across sessions, `for_owner(owner, after)` filters, `EntryView` JSON matches the contracts/api.md example.
- Checks: daemon
- Reviewers: architecture

## WP-04 Activity history wiring and endpoint
- Phase: 2 Foundational
- Tasks: T006, T007
- Files: crates/delilah-daemon/src/state.rs, crates/delilah-daemon/src/hooks.rs, crates/delilah-daemon/src/session_api.rs, crates/delilah-daemon/src/api.rs
- Depends on: WP-02, WP-03
- Parallel group: 2B
- Acceptance: FR-013 — hook events of live Claude sessions are recorded; `Topic::Activity` emitted at once only on `Activity` change, otherwise `activity_dirty` set (`activity_is_announced_on_change_only` still passes); `GET /api/v1/activity/history?owner=&after=` per contract (`started_at`, `live`, `dropped`, entries; 400 on missing owner or bad `after`; unknown owner → empty).
- Checks: daemon
- Reviewers: contract, security, architecture

## WP-05 Token meter and tokens in /activity
- Phase: 2 Foundational
- Tasks: T009, T010
- Files: crates/delilah-daemon/src/transcript_watch.rs, crates/delilah-daemon/src/tokens.rs, crates/delilah-daemon/src/state.rs, crates/delilah-daemon/src/lib.rs, crates/delilah-daemon/src/session_api.rs
- Depends on: WP-02, WP-04
- Parallel group: 2C
- Acceptance: FR-025 — `transcript_progress` replaces `transcript_seen` (newline/Conversations behaviour and tests kept); reads ≤ 1 MB outside the lock and applies only if `offset` unchanged; emits on `total` change and once per tick when dirty; first sight counts existing lines, partial line waits, > 1 MB catches up, concurrent checks count once, shell sessions and out-of-projects transcripts never read; `GET /api/v1/activity` returns `tokens` for live sessions with a meter.
- Checks: daemon
- Reviewers: contract, security, architecture

## WP-06 Client API types for history and tokens
- Phase: 2 Foundational
- Tasks: T011
- Files: app/src/api.ts, app/src/api.test.ts
- Depends on: —
- Parallel group: 2A
- Acceptance: FR-013, FR-025 — `TokenUsage`, `ActivityResponse` (`sessions`, `tokens`), `HistoryEntry` union by `type`, `HistoryResponse`, `api.activityHistory(owner, after?)` with owner URL-encoded; `api.activity()` stays compatible. Built from contracts/api.md (no wait on the daemon).
- Checks: ui
- Reviewers: contract

## WP-07 Art helpers: ramp, outfits, kinds
- Phase: 2 Foundational
- Tasks: T012, T013, T014
- Files: app/src/playground/art/ramp.ts, app/src/playground/art/ramp.test.ts, app/src/playground/art/outfit.ts, app/src/playground/art/outfit.test.ts, app/src/playground/art/kinds.ts, app/src/playground/art/kinds.test.ts
- Depends on: WP-01
- Parallel group: 2A
- Acceptance: `ramp` matches every fixture in `ramp.fixtures.ts`; `lookFor` deterministic (same project → same outfit) with golden splitmix64 seeds; `roleLook` fixed per role; `KIND_COLOURS` as specified; `tokenTier` thresholds 250K/2M/16M; `formatTokens` "816", "9.5K", "2.4M".
- Checks: ui
- Reviewers: —

## WP-08 PixelSprite component
- Phase: 2 Foundational
- Tasks: T015
- Files: app/src/playground/PixelSprite.tsx, app/src/playground/PixelSprite.css, app/src/playground/PixelSprite.test.tsx, app/src/playground/art/pixtuoid.test.ts
- Depends on: WP-01, WP-07
- Parallel group: 2B
- Acceptance: one `<path>` per colour with merged row runs, crispEdges, aria-hidden; recolour applied; animations cycle via CSS `steps()` (frames × frameMs), paused under `.pg-paused`, frame 0 under reduced motion; every generated sprite has equal-width rows, only palette keys and marks inside.
- Checks: ui
- Reviewers: —

## WP-09 US1 Floor layout and useOffice
- Phase: 3 User Story 1
- Tasks: T016, T017
- Files: app/src/playground/floorLayout.ts, app/src/playground/floorLayout.test.ts, app/src/playground/useOffice.ts, app/src/playground/useOffice.test.ts, app/src/playground/layout.ts, app/src/playground/layout.test.ts, app/src/i18n/en/playgroundLayout.ts
- Depends on: WP-06
- Parallel group: 2B (early start approved; phase 3)
- Acceptance: FR-001..FR-004 — slots stable per owner, leads first, freed slots reused, floor grows by pods, leavers kept 5 s, `loungePath` desk → aisle → lounge; `useOffice`/`useFloor(owner)` group live Claude sessions by task key or `@project` with tokens; `deskState` maps Paused → sleeping; accessible names add "1.2M tokens". `usePlayground.ts` stays until WP-13 removes it.
- Checks: ui
- Reviewers: architecture

## WP-10 US1 Floor view
- Phase: 3 User Story 1
- Tasks: T018
- Files: app/src/playground/Floor.tsx, app/src/playground/Floor.css, app/src/playground/Floor.test.tsx, app/src/i18n/en/Floor.ts
- Depends on: WP-07, WP-08, WP-09
- Parallel group: 3B
- Acceptance: FR-002..FR-008 — a desk per lead and helper; monitor glow by tool kind; `?` (`--acc`) and `zzz` (`--pause`) bubbles; token tower by tier with hover text; captions ≥ 12px, 40 chars; lead click/Enter → `onOpenSession`; leavers walk to the lounge then fade (still on the sofa under reduced motion); framed inset with 30px header per R10; scale ×5/×4/×3 to fit; empty state "Ningún agente trabajando en esta tarea"; stale dimming; paused when hidden.
- Checks: ui
- Reviewers: —

## WP-11 US1 Oficina tab
- Phase: 3 User Story 1
- Tasks: T019
- Files: app/src/editor/session.ts, app/src/editor/session.test.ts, app/src/components/TaskTabs.tsx, app/src/components/TaskTabs.css, app/src/components/TaskTabs.test.tsx, app/src/components/TaskScreen.tsx, app/src/components/TaskScreen.test.tsx, app/src/playground/OfficeTab.tsx, app/src/playground/OfficeTab.css, app/src/playground/OfficeTab.test.tsx, app/src/i18n/en/OfficeTab.ts, app/src/i18n/en/TaskTabs.ts, app/src/App.tsx, app/src/App.test.tsx
- Depends on: WP-10
- Parallel group: 3C
- Acceptance: FR-001, FR-009 — fixed, not closable Oficina tab right after Overview on task and project screens, with 13px icon and `--acc` dot when an agent of that owner waits; body renders the owner's `Floor`; `openTask(owner, OFFICE)` opens it.
- Checks: ui
- Reviewers: architecture

## WP-12 US2 FloorMini
- Phase: 4 User Story 2
- Tasks: T020
- Files: app/src/playground/FloorMini.tsx, app/src/playground/FloorMini.css, app/src/playground/FloorMini.test.tsx, app/src/i18n/en/FloorMini.ts
- Depends on: WP-08, WP-09
- Parallel group: 3C (early start approved, alongside WP-11; phase 4)
- Acceptance: FR-010, FR-011 — one button per floor: key, truncated title (full on hover), phase, up to 6 ×2 characters with state and "+N", waiting pill and left accent bar; accessible name "LIBRE-3, Implementación, 2 agentes, 1 esperando".
- Checks: ui
- Reviewers: —

## WP-13 US2 Building, window drill-down and old files removed
- Phase: 4 User Story 2
- Tasks: T021, T022, T023
- Files: app/src/playground/Building.tsx, app/src/playground/Building.css, app/src/playground/Building.test.tsx, app/src/i18n/en/Building.ts, app/src/playground/floorLayout.ts, app/src/playground/floorLayout.test.ts, app/src/playground/PlaygroundView.tsx, app/src/playground/PlaygroundView.test.tsx, app/src/playground/PlaygroundView.css, app/src/i18n/en/PlaygroundView.ts, app/src/playground/PlaygroundWindow.tsx, app/src/playground/PlaygroundWindow.test.tsx, app/src/playground/layout.ts, app/src/playground/Desk.tsx, app/src/playground/Desk.css, app/src/playground/sprites.ts, app/src/playground/sprites.test.ts, app/src/playground/Sprite.tsx, app/src/playground/Sprite.css, app/src/playground/Sprite.test.tsx, app/src/playground/poses.ts, app/src/playground/usePlayground.ts, app/src/i18n/en/Desk.ts, plus any file whose import of a removed module must be fixed
- Depends on: WP-11, WP-12
- Parallel group: 4B
- Acceptance: FR-010..FR-012, SC-002 — floors stacked newest on top in stable order, total waiting counter, ≥ 6 floors fit in a 900px window, empty message, stale banner; main window click → `openTask(owner, OFFICE)`; separate window opens the floor in place with Back, lead click → `requestOpenTask`; old Desk/Sprite/sprites/poses/usePlayground removed; `ui-all` green.
- Checks: ui, ui-all
- Reviewers: architecture

## WP-14 US3 Activity history hook
- Phase: 5 User Story 3
- Tasks: T024
- Files: app/src/playground/useActivityHistory.ts, app/src/playground/useActivityHistory.test.ts
- Depends on: WP-06
- Parallel group: 5A
- Acceptance: FR-013, FR-022 — full fetch on mount, `after=<last seq>` on each `activity` topic, merge per session, full refetch on reconnect; exposes `{ started_at, sessions, stale }`.
- Checks: ui
- Reviewers: —

## WP-15 US3 Flow model and layout
- Phase: 5 User Story 3
- Tasks: T025, T026
- Files: app/src/playground/flowModel.ts, app/src/playground/flowModel.test.ts, app/src/playground/flowLayout.ts, app/src/playground/flowLayout.test.ts
- Depends on: WP-06
- Parallel group: 5A
- Acceptance: FR-014..FR-016 — agent/helper/tool nodes and edges, pairing by tool use id, `failed`, running while live vs `unknown` after end, end without start ignored, parallel calls as separate nodes, notes for `dropped` and empty since `started_at`; layout left-to-right tree with tools in rows, positions depend only on order.
- Checks: ui
- Reviewers: —

## WP-16 US3 Timeline
- Phase: 5 User Story 3
- Tasks: T027
- Files: app/src/playground/Timeline.tsx, app/src/playground/Timeline.css, app/src/playground/Timeline.test.tsx, app/src/i18n/en/Timeline.ts
- Depends on: WP-07, WP-15
- Parallel group: 5B
- Acceptance: FR-017 — rows as buttons (time, kind colour + name, tool, target), selected row highlighted and scrolled into view, keyboard navigation.
- Checks: ui
- Reviewers: —

## WP-17 US3 FlowView and Oficina/Flujo toggle
- Phase: 5 User Story 3
- Tasks: T028, T029
- Files: app/src/playground/FlowView.tsx, app/src/playground/FlowView.css, app/src/playground/FlowView.test.tsx, app/src/i18n/en/FlowView.ts, app/src/playground/OfficeTab.tsx, app/src/playground/OfficeTab.css, app/src/playground/OfficeTab.test.tsx, app/src/i18n/en/OfficeTab.ts, app/src/playground/PlaygroundWindow.tsx, app/src/playground/PlaygroundWindow.test.tsx
- Depends on: WP-13, WP-14, WP-15, WP-16
- Parallel group: 5C
- Acceptance: FR-014..FR-022, FR-027 — SVG graph with pan/zoom (+ / − / Ajustar change viewBox), node styling per R10, running in `--run` (no pulse under reduced motion or while hidden), failed in `--err`, details panel, two-way selection with Timeline, notes and stale dimming; `.seg` toggle (`role="tablist"`) persisted in `localStorage` `delilah.office.view` in try/catch, default Oficina, shared everywhere including the window's in-place floor.
- Checks: ui
- Reviewers: architecture

## WP-18 Docs
- Phase: 6 Polish
- Tasks: T030
- Files: docs/decisions.md, docs/architecture.md, docs/status.md, README.md
- Depends on: WP-05, WP-17
- Parallel group: 6A
- Acceptance: decision #80 revised; new decisions (pixtuoid art superseding #79's rule, tokens from the transcript, Flow without a graph library, building + per-owner floors); architecture covers activity log, token meter, endpoints, playground modules; status and README describe Oficina, Flujo, building and limits.
- Checks: —
- Reviewers: —

## WP-19 Full checks and preview verification
- Phase: 6 Polish
- Tasks: T031, T032
- Files: any file needed to fix a failing check or a failed preview scenario (outside docs/ and README.md)
- Depends on: WP-05, WP-17
- Parallel group: 6A
- Acceptance: SC-001..SC-00x — `cargo test -p delilah-agents -p delilah-daemon` and `ui-all` green; quickstart.md passes in `scripts/preview.sh --fresh` via `ui-verifier` (floor, building, window drill-down, Flujo, reduced motion, light/dark at ~600px and full width, 1x art legible, R10 styling).
- Checks: agents, daemon, ui-all
- Reviewers: spec-compliance, test
