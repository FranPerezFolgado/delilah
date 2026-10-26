# Tasks: Readable background-task notifications in the chat view

**Input**: Design documents from `specs/011-readable-task-notifications/`
**Prerequisites**: [plan.md](plan.md), [spec.md](spec.md), [research.md](research.md), [data-model.md](data-model.md), [contracts/api.md](contracts/api.md), [quickstart.md](quickstart.md)

**Tests**: test-first, as the project's convention. Every implementation task follows a
task that writes failing tests for it. Checks: `cargo test -p delilah-agents`,
`cargo test -p delilah-daemon`, `cd app && npx tsc --noEmit && npx vitest related --run <files>`.

**Organization**: by user story (spec.md): US1 one clear item per notification (P1), US2
read the report, hand-backs included (P1), US3 no raw notification anywhere (P2). US2
builds on US1's item; US3 only needs the foundational phase.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: can run in parallel (different files, no dependency on an unfinished task)
- **[Story]**: US1–US3 from spec.md

---

## Phase 1: Setup

**Purpose**: the new module and the shared formatter, with no behaviour change.

- [ ] T001 Create `crates/delilah-agents/src/transcript/background.rs` as a submodule of `crates/delilah-agents/src/transcript.rs` (`mod background;` with `pub use` of what later tasks export), holding only a `#[cfg(test)] mod tests {}` for now; `cargo test -p delilah-agents` stays green
- [ ] T002 [P] Write failing tests in a new `app/src/format.test.ts` for `duration(secs)` ("0 s", "59 s", "1 min 00 s", "3 min 05 s", negative → "0 s") and `formatTokens(n)` ("950", "1K", "12.3K", "1.2M")
- [ ] T003 Implement T002: create `app/src/format.ts`, moving `duration` out of `app/src/components/BackgroundStrip.tsx` (which now imports it) and `formatTokens` out of `app/src/playground/art/kinds.ts` (which re-exports it from `../../format`); `BackgroundStrip.test.tsx` and the playground tests stay green unchanged

---

## Phase 2: Foundational (the contract on both sides)

**Purpose**: the `background` field every story reads and writes. Blocks all stories.

- [ ] T004 Write failing tests in `crates/delilah-agents/src/transcript/background.rs` (tests module) that serialize `Entry::Marker { marker: BackgroundDone, detail, background: Some(BackgroundInfo { … }) }` to the three JSON shapes of `contracts/api.md` (agent with usage and report, command with `exit_code: 0`, hand-back) and check that `None`/`false` fields are absent, and that a `Compacted` marker with `background: None` serializes exactly as before (no `background` key)
- [ ] T005 Implement T004: `pub struct BackgroundInfo` with `BackgroundSource { Notification, Handback }` and `BackgroundKind { Agent, Command, Task }` (serde lowercase), fields from `data-model.md` with `skip_serializing_if` (`Option::is_none`, `std::ops::Not::not` for the bools), in `background.rs`; add `background: Option<BackgroundInfo>` (`skip_serializing_if = "Option::is_none"`) to `Entry::Marker` in `crates/delilah-agents/src/transcript.rs` and set it to `None` at every existing construction (grep `Entry::Marker {` across `crates/`)
- [ ] T006 [P] Add `export type BackgroundInfo` (exactly as in `contracts/api.md`) to `app/src/api.ts` and `background?: BackgroundInfo` on the `kind: "marker"` member of `ConversationEntry` (~l.428); `npx tsc --noEmit` passes

**Checkpoint**: the field exists end to end, always absent; behaviour unchanged.

---

## Phase 3: User Story 1 — one clear item per finished background task (P1) 🎯 MVP

**Goal**: each notification is one compact item with kind, label, outcome and metrics; nothing of the XML is shown.

**Independent Test**: quickstart US1, and the transcript tests below on notification texts from `crates/delilah-agents/tests/fixtures/stream-background-hooks.jsonl` (lines 22 and 24).

### Tests for User Story 1

- [ ] T007 [P] [US1] Write failing tests in `crates/delilah-agents/src/transcript/background.rs` for `read_notification(block: &str) -> BackgroundInfo`: tags read by name (`task-id`, `status` lowercased, `summary`, `usage` → `subagent_tokens`/`tool_uses`/`duration_ms` as `u64`); summary `Agent "WP-07 update status.md" finished` → kind `agent`, label `WP-07 update status.md`; `Background command "Sleep 25 seconds in background" completed (exit code 0)` → kind `command`, label in quotes, `exit_code: Some(0)`; `(exit code 1)` → `Some(1)`; any other summary → kind `task`, label = first line ≤ 200 chars; missing closing tags (block truncated after `<status>completed`) still give what can be read; non-numeric usage values → `None`; `tool-use-id`, `output-file` and `note` appear nowhere in the result; `source` is `Notification`
- [ ] T008 [P] [US1] Write failing tests in `crates/delilah-agents/src/transcript.rs` (tests module, next to `task_notification_is_a_background_done_marker`) that a `user` line whose text is one notification (with and without `origin.kind == "task-notification"`) gives one `Entry::Marker` with `detail` as today and `background` filled from the block, and that an `origin.kind == "task-notification"` line with no readable block still gives one marker with `detail: None` and `background: Some { source: Notification, kind: Task, .. }`
- [ ] T009 [P] [US1] Write failing tests in a new `app/src/backgroundItems.test.ts` for `outcome(info)`: `failed` → "failed"; `completed` with `exit_code` ≠ 0 → "failed"; `stopped`/`killed` → "stopped"; `completed` (or exit 0) → "ok"; missing or unknown status → "other"
- [ ] T010 [P] [US1] Write failing tests in a new `app/src/components/BackgroundDoneItem.test.tsx`: an agent item renders "Subagente terminado · WP-07 update status.md · 1 min 29 s · 2 herramientas · 12.3K tokens" (singular "1 herramienta"); a command renders "Comando terminado · Sleep 25 seconds in background · código 0"; failed/stopped render "falló"/"detenida" with the warning class, ok renders without it; an unknown status shows the raw word, neutral; missing metrics are left out (no "0", no empty separators); an entry with no `background` (old daemon) renders today's line "Tarea en segundo plano terminada · <detail>"; no text of the form `<task-` appears in any render

### Implementation for User Story 1

- [ ] T011 [US1] Implement T007 in `crates/delilah-agents/src/transcript/background.rs`: `fn tag<'a>(block: &'a str, name: &str) -> Option<&'a str>` (first `<name>` up to its `</name>`, else up to the next `<` tag start or end of block), summary parsing per research R6, usage parsing, `pub(crate) fn read_notification`
- [ ] T012 [US1] Implement T008 in `crates/delilah-agents/src/transcript.rs`: in the task-notification branch of `line_items` (~l.392-405), build `background` with `background::read_notification` on the notification block (from `<task-notification>` to its closing tag or end of text); keep `notification_summary` for `detail`; keep the fallback marker for unreadable blocks
- [ ] T013 [P] [US1] Implement T009 in a new `app/src/backgroundItems.ts`: `export type Outcome = "ok" | "failed" | "stopped" | "other"` and `export function outcome(info: BackgroundInfo): Outcome`
- [ ] T014 [US1] Implement T010 in a new `app/src/components/BackgroundDoneItem.tsx` (+ `BackgroundDoneItem.css`, colours from the existing tokens in `app/src/styles.css`): props `{ entry: Extract<ConversationEntry, { kind: "marker" }>; complete: boolean }`; a single row with an outcome icon, the kind/outcome phrase, label and metrics from `app/src/format.ts`; all strings through `t()`; add their English to a new `app/src/i18n/en/BackgroundDoneItem.ts` and register it the way the other catalogues are registered so `app/src/i18n/coverage.test.ts` passes
- [ ] T015 [US1] Write a failing test in `app/src/components/ConversationView.test.tsx` that a conversation with a `background-done` marker carrying `background` renders the `BackgroundDoneItem` row (label and metrics visible) and that `compacted`/`stopped` markers render as before; then render `background-done` markers through `BackgroundDoneItem` in `app/src/components/ConversationView.tsx` (~l.318-327), passing `conv.complete`

**Checkpoint**: US1 works on its own: notifications are readable one-line items; reports are not yet shown.

---

## Phase 4: User Story 2 — read the sub-agent's report (P1)

**Goal**: expand an item to read the report as Markdown, including reports delivered as hand-backs; each sub-agent appears once.

**Independent Test**: quickstart US2; a conversation with an inline report and one with a hand-back.

### Tests for User Story 2

- [ ] T016 [P] [US2] Write failing tests in `crates/delilah-agents/src/transcript/background.rs` for the report: `result` taken from `<result>` to the **last** `</result>` before `<usage>`/`</task-notification>` (a report quoting `</result>` is kept whole); the five entities `&lt; &gt; &amp; &quot; &#39;` decoded, raw `<name>` kept as text; the placeholder "This agent's report was delivered to you as a message from …" → `report: None, report_in_handback: true`; a report over 16 KB cut on a char boundary (multi-byte text) with `report_truncated: true`; a command without `result` → `report: None`, both flags false
- [ ] T017 [P] [US2] Write failing tests in `crates/delilah-agents/src/transcript/background.rs` for `read_handback(origin: &Value) -> Option<BackgroundInfo>`: the `origin` of a hand-back (`kind: "peer"`, `handback: true`, `from`, `name`, `body` with the "[Subagent hand-back] … The report follows:\n" preamble and two-space indented lines) → `source: Handback`, `kind: Agent`, `task_id` = `from`, `label` = `name`, `status: "completed"`, `report` without the preamble and with one level of two-space indent removed (lines with less indent kept as they are); a body without the preamble is kept whole; `handback` false or `kind` ≠ `peer` → `None`
- [ ] T018 [P] [US2] Write failing tests in `crates/delilah-agents/src/transcript.rs` (tests module): an `isMeta: true` user line with a hand-back `origin` parses to one `Entry::Marker { marker: BackgroundDone, detail: Some(<name>), background: Some(Handback …) }` with id `t:<uuid>:0`; a notification line and a hand-back line with different uuids get different ids; other `isMeta` lines (e.g. a skill's "Base directory for this skill" text) and `isSidechain` lines still give nothing
- [ ] T019 [P] [US2] Write failing tests in `app/src/backgroundItems.test.ts` for `foldBackground(entries)`: a notification with `report_in_handback` and a hand-back with the same `task_id` (hand-back before or after it) give one entry with the notification's id, time and metrics and the hand-back's report and `report_truncated`; two notifications of the same task id (resumed agent) and two hand-backs pair one to one, nearest first; an unmatched hand-back and an unmatched placeholder are returned unchanged; other entries keep their order; the input array is not mutated; folding the same input twice gives equal output
- [ ] T020 [P] [US2] Write failing tests in `app/src/components/BackgroundDoneItem.test.tsx`: an item with a report shows a disclosure button (`aria-expanded="false"`), and clicking it renders the report through `Markdown` (a heading and a list in the report become elements) and a collapse control at the top of the report; clicking again hides it; `report_truncated` shows "(informe recortado)"; `report_in_handback` without report shows "Cargando informe…" when `complete` is false and "Informe no disponible" when true; a command shows no disclosure button; report text `<script>` renders as text

### Implementation for User Story 2

- [ ] T021 [US2] Implement T016 in `crates/delilah-agents/src/transcript/background.rs`: `MAX_REPORT = 16 * 1024`, last-`</result>` extraction, `decode_entities`, placeholder detection, truncation with the crate's existing `truncate` helper in `transcript.rs` (make it `pub(super)` if needed)
- [ ] T022 [US2] Implement T017 in `crates/delilah-agents/src/transcript/background.rs`: `pub(crate) fn read_handback(origin: &Value) -> Option<BackgroundInfo>` (preamble cut at the first "The report follows:\n", de-indent, `MAX_REPORT` cut)
- [ ] T023 [US2] Implement T018 in `crates/delilah-agents/src/transcript.rs` `line_items` (~l.365): before the `isSidechain || isMeta` early return, when the line is not a sidechain, `isMeta` is true and `background::read_handback(&line["origin"])` is `Some`, return one marker built from it (uuid and timestamp read as for other lines); everything else behaves as today
- [ ] T024 [US2] Implement T019 in `app/src/backgroundItems.ts`: `export function foldBackground(entries: ConversationEntry[]): ConversationEntry[]` per `data-model.md`
- [ ] T025 [US2] Implement T020 in `app/src/components/BackgroundDoneItem.tsx`: the disclosure, `<Markdown text={report} remoteImages="click" />` (from `app/src/components/Markdown.tsx`) inside a `selectable` block, the truncated note, the loading/unavailable texts by `complete`; new strings in `app/src/i18n/en/BackgroundDoneItem.ts`
- [ ] T026 [US2] Write a failing test in `app/src/components/ConversationView.test.tsx` that a notification with `report_in_handback` on the first page and its hand-back on a second page (`complete: false`, then `complete: true`) end as one item whose expanded report is the hand-back's, with no second item; then apply `foldBackground` in `app/src/components/ConversationView.tsx` with a `useMemo` over the merged entries (the raw entries stay in state)

**Checkpoint**: US1 + US2: every finished sub-agent is one item with its report one click away.

---

## Phase 5: User Story 3 — no raw notification anywhere else (P2)

**Goal**: unusual shapes and other surfaces never show notification or hand-back markup.

**Independent Test**: the tests below; quickstart US3 step 2 in a preview.

### Tests for User Story 3

- [ ] T027 [P] [US3] Write failing tests in `crates/delilah-agents/src/transcript/background.rs` for `split_notifications(text) -> Vec<Segment>` (`Segment::Text(&str)` / `Segment::Notification(&str)`): two notifications in one text → two `Notification` segments in order; user text before and after a notification → `Text`, `Notification`, `Text`; a notification without its closing tag ends at the next `<task-notification>` or end of text; text without notifications → one `Text`; and for `is_agent_message(text)`: true for text containing `<task-notification>` anywhere and for text starting (after whitespace) with "Another Claude session sent a message", false for ordinary prompts that merely mention "task-notification" without the tag
- [ ] T028 [P] [US3] Write failing tests in `crates/delilah-agents/src/transcript.rs` (tests module): a string-content user line with two notifications gives two markers with ids `t:<uuid>:0` and `t:<uuid>:0:1`; a content array `[text "hola", text "<task-notification>…"]` gives a user entry "hola" (`t:<uuid>:0`) and a marker (`t:<uuid>:1`); a single text block "hola\n<task-notification>…</task-notification>\nadiós" gives user "hola", a marker, user "adiós" with distinct ids; across all these inputs no `Entry::User` text contains `<task-notification` or `<agent-message`
- [ ] T029 [P] [US3] Write a failing test in `crates/delilah-daemon/src/hooks.rs` next to `a_prompt_the_agent_made_up_or_a_task_notification_is_not_the_users_last_message`: a `UserPromptSubmit` whose prompt is "Another Claude session sent a message:\n<agent-message …>", and one whose prompt has text before `<task-notification>`, do not change the session's last message

### Implementation for User Story 3

- [ ] T030 [US3] Implement T027 in `crates/delilah-agents/src/transcript/background.rs`: `pub(crate) enum Segment`, `pub(crate) fn split_notifications`, `pub fn is_agent_message` (re-exported from `transcript`)
- [ ] T031 [US3] Implement T028 in `crates/delilah-agents/src/transcript.rs`: replace the whole-line `starts_with("<task-notification>")` branch with a per-text-block split in `message_items` for user messages (`Text` segments → `Entry::User` unless `is_injected`, `Notification` segments → marker via `read_notification`), ids per `data-model.md`; keep the `origin.kind == "task-notification"` fallback marker when no block is readable; have `is_injected` call `background::is_agent_message` so the rule has one home
- [ ] T032 [US3] Implement T029 in `crates/delilah-daemon/src/hooks.rs` `agent_prompt` (~l.258): use `delilah_agents::transcript::is_agent_message` for the prompt test (keep the `delilah_origin == "agent"` check)

**Checkpoint**: all stories done; no raw notification in any shape or surface covered by tests.

---

## Phase 6: Polish & cross-cutting

- [ ] T033 [P] Update `docs/status.md` (chat mode: background notifications are readable items with expandable reports, hand-backs included; known limit: when Claude Code drops a notification in favour of a hand-back, no metrics are shown; reports cut at 16 KB) and `docs/usage.md` (chat view: what a finished background task looks like, how to read its report)
- [ ] T034 [P] Add decision **97** to `docs/decisions.md` as drafted in plan.md D4 (marker + optional `background` object and why not a new kind; hand-backs as items; client-side fold derived on render; 16 KB cap, no detail endpoint; ruled out: XML library, `queue-operation` lines, server-side pairing), and the line about notifications in `docs/architecture.md` where transcript entries/markers are described, if it lists them
- [ ] T035 Run `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --check`, and `cd app && npx tsc --noEmit && npx vitest run`; fix what fails
- [ ] T036 Run quickstart.md in a preview (`scripts/preview.sh --fresh`, never the installed daemon): US1 steps 1–5, US2 steps 1–2, US3 step 2, and the compatibility check with the `main` UI; record the result in `specs/011-readable-task-notifications/quickstart.md` under a "Results" heading

---

## Dependencies & execution order

- **Setup (T001–T003)**: no dependencies; T001 and T002/T003 in parallel.
- **Foundational (T004–T006)**: after T001; T006 in parallel with T004/T005. Blocks every story.
- **US1 (T007–T015)**: after Foundational. Rust (T007, T008 → T011, T012) and UI (T009, T010 → T013, T014 → T015) run in parallel; T014 needs T003 and T013.
- **US2 (T016–T026)**: after US1's T011/T012 (Rust side) and T014 (UI side). Rust (T016–T018 → T021–T023) and UI (T019, T020 → T024, T025 → T026) in parallel.
- **US3 (T027–T032)**: after Foundational and T011 (`read_notification`); independent of US2 except that T031 and T023 both edit `transcript.rs`: run them one after the other.
- **Polish (T033–T036)**: after the stories it documents; T033/T034 in parallel; T035 then T036 last.

### Files shared between tasks (no two writers at once)

- `crates/delilah-agents/src/transcript.rs`: T005, T008/T012, T018/T023, T028/T031
- `crates/delilah-agents/src/transcript/background.rs`: T004/T005, T007/T011, T016/T021, T017/T022, T027/T030
- `app/src/components/BackgroundDoneItem.tsx`: T014, T025
- `app/src/components/ConversationView.tsx` (+ test): T015, T026
- `app/src/backgroundItems.ts` (+ test): T009/T013, T019/T024

## Parallel examples

- **US1**: one agent does T007 → T011 → T008 → T012 (Rust); another does T009 → T013 → T010 → T014 → T015 (UI).
- **US2**: one agent does T016 → T021 → T017 → T022 → T018 → T023 (Rust); another does T019 → T024 → T020 → T025 → T026 (UI).
- **US3** Rust work can follow US2's Rust package in the same agent; T029/T032 (`hooks.rs`) can run in parallel with any UI package.

## Implementation strategy

1. **MVP**: Setup + Foundational + US1. Raw XML is gone and every notification is one readable line with its outcome, which already answers the user's report.
2. **US2** adds the reports and hand-backs, the main value for orchestrated sessions.
3. **US3** closes the uncommon shapes and the last-message surface.
4. Polish: docs, decision, full checks, preview run. No version bump on this branch.
