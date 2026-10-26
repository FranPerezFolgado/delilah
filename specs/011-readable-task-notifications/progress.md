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
| WP-09 | 5 | 5A | verified | 1 | | |
| WP-10 | 5 | 5B | verified | 1 | | |
| WP-11 | 6 | 6A | verified | 1 | | |
| WP-12 | 6 | 6B | pending | 0 | | |

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
