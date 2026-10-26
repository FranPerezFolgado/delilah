# Progress — 011-readable-task-notifications

Branch: 011-readable-task-notifications · Base: 3981337 · Started: 2026-10-26
Mode: format=markdown commits=yes autonomy=auto

| WP | Phase | Group | Status | Attempts | Commit | Notes |
|----|-------|-------|--------|----------|--------|-------|
| WP-01 | 1 | 1A | done | 1 | | |
| WP-02 | 1 | 1A | done | 1 | | |
| WP-03 | 2 | 2A | done | 1 | | |
| WP-04 | 2 | 2A | done | 1 | | |
| WP-05 | 3 | 3A | done | 2 | | |
| WP-06 | 3 | 3A | done | 1 | | also edited i18n/en/ConversationView.ts (stale key moved), allowed |
| WP-07 | 4 | 4A | done | 1 | | |
| WP-08 | 4 | 4A | done | 1 | | |
| WP-09 | 5 | 5A | done | 1 | | |
| WP-10 | 5 | 5B | done | 1 | | |
| WP-11 | 6 | 6A | done | 1 | | |
| WP-12 | 6 | 6B | done | 1 | | T035 green (1 flaky pre-existing daemon test passed on re-run) |
| WP-09F | 5 | 5F | done | 1 | | phase-5 review fixes (linear split, trim only split blocks) |
| WP-12F | 6 | 6F | done | 1 | | preview fixes: mid-turn attachment notifications, `with exit code N` |

## Open findings

| Phase | Reviewer | Severity | Finding | File |
|-------|----------|----------|---------|------|
| 1-2 | contract | LOW | No shared Rust↔TS JSON fixture for `background` (parity is manual) | app/src/api.ts |
| 1-2 | contract | LOW | `Default` on wire enums BackgroundSource/BackgroundKind can hide a missing choice | crates/delilah-agents/src/transcript/background.rs |
| 1-2 | spec | LOW | T004 hand-back serialization test doesn't match the contract's hand-back JSON shape exactly | crates/delilah-agents/src/transcript/background.rs |
| 3 | security | LOW | Quoted agent/command labels not capped at 200 chars (data-model says ≤ 200) — handed to WP-07 | crates/delilah-agents/src/transcript/background.rs |
| 3 | security | LOW | User text starting with `<task-notification>` without origin becomes a marker (pre-existing, cosmetic) | crates/delilah-agents/src/transcript.rs |
| 3 | spec | MEDIUM | Status-less / unknown-status notifications lack the "terminada" wording — handed to WP-08 | app/src/components/BackgroundDoneItem.tsx |
| 3 | spec | LOW | No end-to-end test parses the real stream-background fixtures through `parse` | crates/delilah-agents/src/transcript.rs |
| 3 | spec | LOW | T015 test doesn't assert compacted/stopped markers unchanged (covered by older tests) | app/src/components/ConversationView.test.tsx |
| 4 | security | LOW | Typed user text with `<task-notification>` renders as a fake finished item (by design of FR-009; cosmetic) | crates/delilah-agents/src/transcript.rs |
| 4 | security | LOW | `task_id` (hand-back `from`, `<task-id>`) not length-capped — handed to WP-09 | crates/delilah-agents/src/transcript/background.rs |
| 4 | spec | LOW | `read_report` fallback/rfind not bounded by `<usage>`/`</task-notification>` on malformed input — handed to WP-09 | crates/delilah-agents/src/transcript/background.rs |
| 4 | spec | LOW | No render test for a lone hand-back item or a hand-back with empty body | app/src/components/BackgroundDoneItem.test.tsx |
| 5 | security | MEDIUM | `is_agent_message` matches `<task-notification>` anywhere (as T027/T029 require), so a real prompt that pastes the tag is treated as an agent prompt in hooks (no last message, early return on turn start/held Stop) — spec-settled, left for the user to decide | crates/delilah-daemon/src/hooks.rs |
| 5 | security | LOW | `split_notifications` O(N·len) on many unclosed tags — FIXED in WP-09F | crates/delilah-agents/src/transcript/background.rs |
| 5 | spec | LOW | Ordinary user text now trimmed even without notifications — FIXED in WP-09F | crates/delilah-agents/src/transcript.rs |
| 5 | spec | LOW | Mixed prompt "hola\n<task-notification>…" drops "hola" as last message (as T029 asks) | crates/delilah-daemon/src/hooks.rs |
| 5 | spec | LOW | Non-hand-back "Another Claude session…" messages now vanish from chat (as T031 asks), undocumented | docs/usage.md |
| 5 | spec | LOW | `origin.kind` fallback marker has `detail: None` and no new test | crates/delilah-agents/src/transcript.rs |
| 6 | preview | LOW | If Claude Code ever writes the same notification both as a `queued_command` attachment and as a user line, it would show twice (not seen: absorbed mid-turn notifications are not repeated) | crates/delilah-agents/src/transcript.rs |
| 6 | preview | LOW | UI rendering not checked in a browser (no browser tools); covered by vitest only | app/src/components/BackgroundDoneItem.tsx |
| final | quality | MEDIUM | An unclosed `<task-notification>` typed mid-sentence swallows the rest of the user's text (spec: truncated blocks keep what can be read; left as is, same trade-off as the hooks check) | crates/delilah-agents/src/transcript/background.rs |
| final | quality | LOW | `formatTokens` showed "1000K" just under one million — FIXED | app/src/format.ts |
| final | quality | LOW | Phrase choice was three nested ternaries — FIXED (table by kind and outcome) | app/src/components/BackgroundDoneItem.tsx |
| final | architecture | LOW | `formatTokens` re-export shim in playground `kinds.ts` — FIXED (importers use `format.ts`) | app/src/playground/art/kinds.ts |
| final | architecture | LOW | Docs quoted UI strings only in Spanish; hidden "Another Claude session" messages undocumented — FIXED | docs/usage.md, docs/status.md |
