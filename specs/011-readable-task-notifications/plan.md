# Implementation Plan: Readable background-task notifications in the chat view

**Branch**: `011-readable-task-notifications` | **Date**: 2026-10-26 | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `specs/011-readable-task-notifications/spec.md`

## Summary

`main` (PR #30) already turns a transcript line that *starts with* `<task-notification>`
into a one-line `background-done` marker, keeping only the first summary line. This plan
keeps that marker and gives it the whole notification, and it closes the gaps found in
real transcripts ([research.md](research.md)):

1. **Transcript parser** (`delilah-agents::transcript`): a tolerant tag scanner reads
   every notification in a user text block (status, summary → kind/label/exit code, usage,
   report with entities decoded). It splits mixed text, so user text stays a bubble and
   each notification becomes its own marker. Hidden **hand-back** lines
   (`origin.kind == "peer"`, `handback: true`) become markers too, with the report taken
   from `origin.body` without the harness preamble. The marker gains an optional
   `background` object; `detail` stays for old clients.
2. **Hooks** (`delilah-daemon::hooks::agent_prompt`): the same detection, so a
   notification or hand-back prompt is never the task's "last user message".
3. **UI** (`app/`): `api.ts` types `background`. A pure `foldBackground()` merges a
   notification whose report came as a hand-back with that hand-back (across pages). A new
   `BackgroundDoneItem` shows one compact line (icon by outcome, label, metrics), and the
   report expands as `Markdown`. `ConversationView` uses both. Duration/token formatting
   moves to a shared `format.ts`, which `BackgroundStrip` also uses.
4. **Docs**: `docs/status.md`, `docs/usage.md`, decision 97 in `docs/decisions.md`.

No migration, no new endpoint, no new dependency.

## Technical Context

**Language/Version**: Rust 2024 (workspace), TypeScript + React 18 (Vite)
**Primary Dependencies**: serde/serde_json (daemon); react-markdown via the existing `Markdown` component (UI)
**Storage**: none (derived from Claude Code transcripts on read)
**Testing**: `cargo test -p delilah-agents`, `cargo test -p delilah-daemon`; vitest + Testing Library in `app/`
**Target Platform**: macOS desktop app (Tauri) and the same UI in a browser preview
**Project Type**: daemon + desktop UI
**Performance Goals**: parsing stays linear in the transcript size; a collapsed item is one line (SC-004)
**Constraints**: additive contract (FR-012); Spanish UI strings through `t()` with an EN catalogue entry (`app/src/i18n/en/*.ts`, coverage test); reports ≤ 16 KB per entry
**Scale/Scope**: sessions with tens of background tasks; reports up to tens of KB

## Constitution Check

`.specify/memory/constitution.md` is the unfilled template; the project's rules are in
`CLAUDE.md` and `docs/development.md`. Gates:

| Gate | Status |
|---|---|
| Contracts change on both sides together (`api.rs`/entry serde ↔ `app/src/api.ts`) | ✅ one field, both sides in this plan |
| Never edit a shipped migration | ✅ no migration |
| Try changes only in a preview, never the installed daemon | ✅ quickstart uses `scripts/preview.sh` |
| No private repo data in fixtures | ✅ new fixtures are synthetic or from the existing fixture repos |
| Docs kept true (status, usage, decisions) | ✅ phase "Docs" |
| No version bump on the feature branch | ✅ none planned |

Re-checked after design: no violations.

## Project Structure

### Documentation (this feature)

```text
specs/011-readable-task-notifications/
├── spec.md
├── plan.md              # this file
├── research.md          # R1–R7
├── data-model.md
├── quickstart.md
├── contracts/api.md
├── checklists/requirements.md
└── tasks.md             # /speckit-tasks
```

### Source Code (repository root)

```text
crates/delilah-agents/
├── src/transcript.rs                       # Entry::Marker gains `background`; line_items/message_items call the submodule
└── src/transcript/background.rs            # new: BackgroundInfo, scanner, segment split, hand-back reading, injected-prompt rule, tests

crates/delilah-daemon/
└── src/hooks.rs                            # agent_prompt detection + test

app/src/
├── api.ts                                  # BackgroundInfo type on the marker entry
├── format.ts (+ format.test.ts)            # new: duration() from BackgroundStrip, formatTokens() from playground/art/kinds.ts
├── playground/art/kinds.ts                 # re-exports formatTokens from format.ts
├── backgroundItems.ts (+ .test.ts)         # new: foldBackground(), outcome()
├── components/BackgroundDoneItem.tsx (+ .css, .test.tsx)   # new
├── components/BackgroundStrip.tsx          # use format.ts
├── components/ConversationView.tsx (+ test)  # fold + render the item
└── i18n/en/BackgroundDoneItem.ts           # new EN strings

docs/status.md, docs/usage.md, docs/decisions.md
```

**Structure Decision**: parsing stays in `delilah-agents::transcript`, the one mapping
from transcript to entries that every reader (task chat, PTY sessions in the chat view,
idea conversations) already uses; `conversation.rs` serializes entries as they come and
needs no change. Pairing hand-backs is UI-side because only the client holds all pages.

## Design

### D1. Parser (`transcript.rs` + new `transcript/background.rs`)

`transcript.rs` is already 1,400+ lines with inline tests, so the notification and
hand-back code lives in a submodule `transcript/background.rs` (with its own tests);
`transcript.rs` only gains the `background` field and two calls.

- `pub struct BackgroundInfo` (fields in [data-model.md](data-model.md)), `Serialize` with
  `skip_serializing_if` on every optional field; `Entry::Marker` gains
  `background: Option<BackgroundInfo>` (skipped when `None`, so `compacted`/`stopped`
  markers serialize as today).
- `fn split_notifications(text: &str) -> Vec<Segment>` with
  `enum Segment { Text(&str), Notification(&str) }`: finds each `<task-notification>`;
  the block ends at the next `</task-notification>` or at the next `<task-notification>` /
  end of text when the closing tag is missing.
- `fn read_notification(block: &str) -> BackgroundInfo`: `tag(block, "status")`,
  `tag(block, "summary")`, `tag(block, "task-id")`, the usage numbers, `result` (last
  `</result>`), `decode_entities()` for `&lt; &gt; &amp; &quot; &#39;`. Summary → kind,
  label, exit code (R6). Placeholder result ("delivered to you as a message") →
  `report: None, report_in_handback: true`. Reports are cut at `MAX_REPORT = 16 * 1024`
  on a char boundary (reuse `truncate`).
- `line_items`: before the `isSidechain || isMeta` early return, a hand-back line
  (`isMeta`, `origin.kind == "peer"`, `origin.handback == true`, not a sidechain) yields
  one marker from `origin` (R2: preamble cut at "The report follows:\n", one two-space
  indent removed per line). Other meta lines stay hidden.
- The current `from_origin || starts_with` branch is replaced: for user lines, every text
  block goes through `split_notifications`. `Text` segments go through the existing
  `is_injected` test and become `Entry::User`. `Notification` segments become markers.
  Ids follow the data model (`t:<uuid>:<n>` then `t:<uuid>:<n>:<k>`). A line with
  `origin.kind == "task-notification"` whose text contains no readable block still yields
  one marker with only `detail: None` and `background: { source: notification, kind: task }`
  (today's behaviour).
- `notification_summary` stays as the source of `detail`.
- Tests (in the submodule, inline strings like the existing transcript tests; no new
  fixture file): each shape in R1–R3, the serialization in contracts/api.md, a hand-back
  line and a notification line with distinct uuids giving distinct ids, and a check that
  no `Entry::User` text contains `<task-notification` or `<agent-message` for any of those
  inputs. The existing stream fixtures supply real notification texts.

### D2. Hooks (`hooks.rs`)

`agent_prompt` → also true when the prompt contains `<task-notification>` anywhere or
starts with "Another Claude session sent a message". The rule has one home:
`transcript::is_agent_message(&str) -> bool` in the submodule, used by `hooks::agent_prompt`
and by the transcript's own `is_injected` (which keeps its other prefixes), so the
"text the user did not type" rules live together in `transcript`. Test
next to the existing `a_prompt_the_agent_made_up_or_a_task_notification…`.

### D3. UI

- `api.ts`: `BackgroundInfo` type; the marker member of `ConversationEntry` gains
  `background?: BackgroundInfo`.
- `format.ts`: `duration(secs)` moved from `BackgroundStrip` ("12 s" / "3 min 05 s") and
  `formatTokens(n)` moved from `playground/art/kinds.ts` ("950" / "12.3K" / "1.2M"), which
  re-exports it. No behaviour change for either caller; their tests keep passing.
- `backgroundItems.ts`: `foldBackground(entries)` (data-model) and
  `outcome(info): "ok" | "failed" | "stopped" | "other"` (R6).
- `BackgroundDoneItem.tsx`: one row: outcome icon + "Subagente terminado / Comando
  terminado / Tarea en segundo plano terminada" (or "falló" / "detenida") · label ·
  metrics (`duration · N herramientas · NK tokens`, `código N` for commands). Warning
  style for failed/stopped. When there is a report: a disclosure button (`aria-expanded`),
  collapsed by default, that renders `<Markdown text={report} remoteImages="click" />`
  in a block with its own collapse control at the top, plus "(informe recortado)" when
  truncated. `report_in_handback` with no report after the fold → "Cargando informe…"
  while the conversation is not `complete`, "Informe no disponible" once it is.
  Without `background` (old daemon) → today's text line from `detail`.
- `ConversationView.tsx`: render the list through `foldBackground` (a `useMemo` on the
  entries; the raw entries stay in state, so later pages complete pairs on re-render) and
  use `BackgroundDoneItem` for `background-done` markers, passing `complete`. The `compacted` /
  `stopped` rendering is unchanged.
- i18n: new strings in `app/src/i18n/en/BackgroundDoneItem.ts`; the coverage test lists
  them.

### D4. Docs

- `docs/status.md`: chat mode line, background notifications are readable items with
  reports; known limit: when Claude Code drops a notification in favour of a hand-back, no
  metrics are shown.
- `docs/usage.md`: chat view section: what the item shows, expand to read the report.
- `docs/decisions.md` **97**: notifications stay `background-done` markers with an
  optional `background` object. Not a new entry kind, because UIs before this change render
  unknown kinds as cards; the cost is a marker variant with a payload its siblings don't use.
  Hand-backs become items; pairing is client-side, derived on render, because the
  conversation is paginated. Reports capped at 16 KB with no detail endpoint. Ruled out: XML library, reading `queue-operation` lines,
  server-side pairing.

## Plan review

`architecture-reviewer`, round 1 (no CRITICAL):

- HIGH, pairing across pages → the fold is derived on every render from the raw entries
  (the UI loads all pages forward until `complete`); "not available" only once complete.
- HIGH, ids → `arrived()` only matches `claude` entries; test for distinct ids of
  hand-back and notification lines added.
- MEDIUM, marker overload → kept, with the real reason (unknown kinds render as cards in
  existing UIs) recorded in R4 and decision 97.
- MEDIUM, 64 KB inline reports → cap lowered to 16 KB; detail endpoint noted as the next
  step if needed.
- MEDIUM, duplicate token formatter → `formatTokens` moved to `format.ts` and re-exported.
- MEDIUM, two homes for the "injected" rule → one `is_agent_message` in `transcript`.
- LOW, `transcript.rs` size → submodule `transcript/background.rs`; new fixture file dropped.

## Complexity Tracking

No gate violations.
