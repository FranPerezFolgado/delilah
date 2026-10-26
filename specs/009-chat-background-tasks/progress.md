# Progress — 009-chat-background-tasks

Branch: 009-chat-background-tasks · Base: a618cac · Started: 2026-10-26
Mode: format=markdown commits=yes autonomy=auto

| WP | Phase | Group | Status | Attempts | Commit | Notes |
|----|-------|-------|--------|----------|--------|-------|
| WP-01 | 1 | 1A | done | 1 | d931873 | stop_task accepted; Stop + UserPromptSubmit fire |
| WP-02 | 2 | 2A | done | 1 | be15eb0 | |
| WP-03 | 2 | 2A | done | 1 | ef8a45f | |
| WP-04 | 2 | 2B | done | 1 | 79ba591 | |
| WP-05 | 2 | 2C | done | 1 | d6cd429 | tick test real-time (no tokio test-util) |
| WP-06 | 3 | 3A | done | 1 | fddd363 | PreToolUse via hook_event_name (classify → Other) |
| WP-07 | 3 | 3A | done | 1 | 20cb4c1 | live.ts emitter exported, view uses own socket |
| WP-05F | 2 | fix | done | 1 | ed96262 | phase 1-2 review mediums |
| WP-07F | 3 | fix | done | 1 | 8610596 | live.rs busy loop when progress sender drops; live.ts wiring goes in WP-08 |
| WP-08 | 4 | 4A | done | 1 | f8492bc | live.ts progress helper removed; duration() copied |
| WP-09 | 5 | 5A | done | 1 | d69abec | +delilah-core task.rs (Event::AgentResumed) allowed |
| WP-10 | 5 | 5A | done | 1 | 25bffc7 | |
| WP-11 | 6 | 6A | implementing | 0 | | +phase-5 arch MEDIUM (mid-turn notification side effects) |
| WP-13 | 6 | 6A | verified | 1 | 7e87d3c | also refuses POST /sessions/{id}/runtime |
| WP-15 | 6 | 6A | verified | 1 | SHA15 | |
| WP-12 | 6 | 6B | pending | 0 | | |
| WP-14 | 6 | 6B | pending | 0 | | |
| WP-16 | 6 | 6B | pending | 0 | | |
| WP-17 | 7 | 7A | pending | 0 | | |
| WP-18 | 7 | 7B | pending | 0 | | |

## Open findings

| Phase | Reviewer | Severity | Finding | File |
|-------|----------|----------|---------|------|
| 1-2 | contract | LOW | Progress has no golden JSON test pinned to contracts/api.md once api.rs exposes it | crates/delilah-daemon/src/progress.rs |
| 1-2 | contract | LOW | Step/reason strings are hardcoded Spanish in the daemon (client can't localise) | crates/delilah-daemon/src/progress.rs |
| 1-2 | architecture | LOW | Progress cloned several times per event (final_rows_wait, apply before/after, send) | crates/delilah-daemon/src/stream_events.rs |
| 1-2 | architecture | LOW | spawn_pump silently drops progress if the session isn't inserted yet; check insert-before-spawn in run.rs/conversation.rs | crates/delilah-daemon/src/stream_events.rs |
| 1-2 | architecture | LOW | tick on/off via ticker.reset + flag is subtle; sleep_until would be simpler | crates/delilah-daemon/src/stream_events.rs |
| 1-2 | architecture | LOW | StreamEvent is a public non-#[non_exhaustive] enum; Init gained a field | crates/delilah-agents/src/stream.rs |
| 1-2 | spec | LOW | Fixture home paths rewritten to /home/... instead of /fixture/... | crates/delilah-agents/tests/fixtures/stream-background*.jsonl |
| 1-2 | spec | LOW | Fixture interleaving test only counts MessageDone, doesn't check the main message is unchanged | crates/delilah-agents/src/stream.rs |
| 3 | contract | MEDIUM | Old UI renders unknown marker (background-done) as "Detenida"; only if UI/daemon versions mix — mention in release notes | app/src/components/ConversationView.tsx (pre-feature) |
| 3 | contract | LOW | handleSessionMessage / on(id,"progress") in live.ts unused in production (view parses its own socket) | app/src/live.ts |
| 3 | contract | LOW | BackgroundItem.kind typed "command"\|"agent"\|string collapses to string | app/src/api.ts |
| 3 | spec | LOW | Reload test checks the clock after load only, not against the pre-reload value | app/src/components/ConversationView.test.tsx |
| 3 | ui-verifier | NOT CHECKED | US1 scenarios 1-5 verified via API on a real chat session only; no browser tools in this environment, UI render not seen | app/src/components/WorkingIndicator.tsx |
| 4 | spec | LOW | Finished sub-agent duration rounded to whole minutes (min 1): 15 s shows "1 min" | app/src/components/ToolCard.tsx |
| 4 | spec | LOW | Expanded step list shows summary only, not tool name (fine while summaries carry the verb) | app/src/components/ToolCard.tsx |
| 5 | architecture | MEDIUM | Older daemon reading history with Event::AgentResumed fails serde for the whole history (downgrade/mixed versions only) — accepted, to note in docs/decisions.md | crates/delilah-core/src/task.rs |
| 5 | architecture | LOW | "delilah_origin":"agent" is a stringly-typed protocol across 4 files; add a constructor next to agent_prompt | crates/delilah-daemon/src/hooks.rs |
| 5 | architecture | LOW | "background-done" reason code duplicated as literals (progress, stream_events, UI) | crates/delilah-daemon/src/stream_events.rs |
| 5 | architecture | LOW | Paused (limit) session self-start: AgentResumed logged without apply_signal, untested | crates/delilah-daemon/src/stream_events.rs |
| 6 | review-note | LOW | Existing BUSY runtime refusal is hardcoded English, not via lang | crates/delilah-daemon/src/runtime_api.rs |
| 5 | spec | LOW | US4-2 (self-started turn's end applies usual effects) has no specific test | crates/delilah-daemon/src/stream_events.rs |
| 5 | spec | LOW | Divider only when transcript has a background-done marker; other self-started reasons get history only (consistent with "when known") | app/src/components/ConversationView.tsx |
