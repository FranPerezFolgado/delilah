# Progress — 006-chat-view

Branch: feature/LIBRE-13 · Base: a40dd47 · Started: 2026-10-24
Mode: format=markdown commits=yes autonomy=auto

| WP | Phase | Group | Status | Attempts | Commit | Notes |
|----|-------|-------|--------|----------|--------|-------|
| WP-01 | 1 | 1A | done | 1 | a7118d8 | |
| WP-02 | 2 | 2A | implementing | 1 | | T002 done; waiting for the user to run the scenarios (T003/T004) |
| WP-03 | 3 | 3A | done | 1 | 7a74a83 | |
| WP-04 | 3 | 3A | done | 1 | 95b1959 | |
| WP-05 | 4 | 4A | done | 1 | dfd483e | |
| WP-06 | 4 | 4B | done | 1 | f357cc2 | |
| WP-07 | 4 | 4B | done | 1 | 2561634 | |
| WP-05F | 4 | 4B | done | 1 | 693ba38 | security fixes |
| WP-06F | 4 | 4C | done | 1 | f5680ab | architecture fixes |
| WP-08 | 4 | 4C | done | 1 | c3d82e0 | |
| WP-09 | 4 | 4D | done | 1 | c8157ed | |
| WP-11 | 4 | 4D | done | 1 | 5e51e60 | |
| WP-10 | 4 | 4E | done | 1 | c8157ed | run together with WP-09 (shared TaskScreen.tsx) |
| WP-09F | 4 | 4F | done | 1 | 4e689e7 | open-file path fix |
| WP-12 | 5 | 5A | done | 1 | e3f3c57 | |
| WP-12F | 5 | 5C | done | 1 | 806cfd7 | review fixes |
| WP-12G | 5 | 5D | done | 1 | 2e6af70 | re-review fixes |
| WP-13 | 5 | 5B | done | 1 | 626cd52 | |
| WP-14 | 6 | 6A | pending | 0 | | gated by spike |
| WP-15 | 6 | 6B | pending | 0 | | gated by spike |
| WP-16 | 7 | 7A | pending | 0 | | gated by spike |
| WP-17 | 7 | 7A | pending | 0 | | gated by spike |
| WP-18 | 7 | 7B | pending | 0 | | gated by spike |
| WP-19 | 7 | 7C | pending | 0 | | gated by spike |
| WP-20 | 7 | 7C | pending | 0 | | gated by spike |
| WP-21 | 7 | 7D | pending | 0 | | gated by spike |
| WP-22 | 8 | 8A | pending | 0 | | gated by spike |
| WP-23 | 8 | 8B | pending | 0 | | gated by spike |
| WP-24 | 8 | 8C | pending | 0 | | gated by spike |
| WP-25 | 8 | 8D | pending | 0 | | only if d2=no |
| WP-26 | 9 | 9A | pending | 0 | | gated by spike |
| WP-27 | 9 | 9B | pending | 0 | | gated by spike |
| WP-28 | 9 | 9B | pending | 0 | | gated by spike |
| WP-29 | 9 | 9B | pending | 0 | | gated by spike |
| WP-30 | 10 | 10A | pending | 0 | | |
| WP-31 | 10 | 10B | pending | 0 | | |

## Open findings

| Phase | Reviewer | Severity | Finding | File |
|-------|----------|----------|---------|------|
| 5 | architecture | LOW | WP-12G adds a process-wide static NOT_FOUND map (session, tool_use_id) → same smell as the old SEEN static; move to AppState | crates/delilah-daemon/src/conversation.rs |
| 5 | ui | LOW | UI rendering not verified in a browser (scroll-follow, Edit diff + Abrir en el editor, Chat/Terminal switch, Vista select, idea Nueva tarea); API-level US1/US2 verified | app/ |
| 5 | spec | LOW | research.md R5 stale: code hides only delivered user entries (doc'd in contracts) — update R5 in docs phase | specs/006-chat-view/research.md |
| 5 | spec | LOW | weak tests: Detener only tested on idea session; card-answer test's conditional submit click; daemon task tests use a project session; stop_refuses_… misnamed; no InboxView refresh test | app/src/components/ConversationView.test.tsx |
| 5 | security | LOW | pending-ask check not serialized with ask hooks (pre-existing race, window narrowed) | crates/delilah-daemon/src/conversation.rs:466 |
| 5 | contract | MEDIUM | idle send: claimed queued + new entries stay `sent` if type_prompt fails (now visible for ideas too); revert to queued on error | crates/delilah-daemon/src/conversation.rs:489 |
| 5 | contract | LOW | brief double show of a message between transcript write and its prompt hook (sent + t: user) | crates/delilah-daemon/src/conversation.rs:259 |
| 5 | contract | LOW | old cards (before e3f3c57) have no tool_use_id → existing idea conversations show tool + card | crates/delilah-daemon/src/conversation.rs:260 |
| 5 | security | MEDIUM | API has no token: any local process without Origin can type prompts into any open Claude session (design risk; document or add token) | crates/delilah-daemon/src/api.rs:194 |
| 4 | contract | MEDIUM | runtime hardcoded "pty"; Task/Session JSON lack runtime/chat_mode/allowed_tools (chat-mode phases) | crates/delilah-daemon/src/conversation.rs:297 |
| 4 | contract | LOW | Task.view optional in api.ts though always sent | app/src/api.ts:63 |
| 4 | contract | LOW | ask_kind typed but never emitted yet (chat-mode permissions) | app/src/api.ts |
| 4 | contract | LOW | stored id stringify via Value::to_string relies on numeric JSON id; keep a test asserting "5" | crates/delilah-daemon/src/conversation.rs:247 |
| 4 | contract | LOW | no test asserting the exact key set of the conversation response | crates/delilah-daemon/src/conversation.rs |
| 4 | spec | LOW | "Vista" select hidden for origin "Con Claude" (FR-007) — sends chat | app/src/components/NewTaskDialog.tsx:319 |
| 4 | spec | LOW | Working session without transcript file yet reads "missing" (only Starting = pending) → notice may flash | crates/delilah-daemon/src/conversation.rs |
| 4 | architecture | LOW | parse_view duplicates validation outside TaskView (string round-trip) | crates/delilah-daemon/src/api.rs:290 |
| 4 | architecture | LOW | conversation.rs grows: split file I/O/bounds into a submodule when next touched | crates/delilah-daemon/src/conversation.rs |
| 4 | migration | LOW | unknown stored view silently reads as chat; optional tracing::warn | crates/delilah-daemon/src/store.rs:455 |
| 4 | migration | LOW | no test applying 0016 over existing 0015 rows | crates/delilah-daemon/migrations/0016_task_view.sql |
| 4 | security | MEDIUM | hook transcript_path accepted anywhere (absolute .jsonl, no ..; symlinks pass) → served via GET conversation; restrict canonical path to ~/.claude/projects, re-check at read | crates/delilah-daemon/src/hooks.rs:151 |
| 4 | security | MEDIUM | every GET reads the whole transcript (no size cap, no is_file); orphan path re-parses whole file; tool_detail parses every line — seek from cursor, cap size, cache | crates/delilah-daemon/src/conversation.rs:82 |
| 4 | security | LOW | any hook overwrites the live transcript path before validation (integrity side of the above) | crates/delilah-daemon/src/hooks.rs:46 |
| 4 | security | LOW | tool-detail response size uncapped (consider 1 MB) | crates/delilah-daemon/src/conversation.rs:166 |
| 3 | spec | LOW | "Delilah's own injected text → skipped" (R3) not implemented; no marker exists yet | crates/delilah-agents/src/transcript.rs:313 |
| 3 | spec | LOW | parse_message pairs results only within one message; stream-json sends tool_result in a separate user message (needs own pairing for FR-016) | crates/delilah-agents/src/transcript.rs:214 |
| 3 | spec | LOW | untested: MultiEdit/Write diffs, Glob/NotebookEdit/Agent summaries, input truncation, tool_detail is_error, missing uuid/bad timestamp | crates/delilah-agents/src/transcript.rs |
| 3 | security | MEDIUM | diff computed on untruncated old/new before truncation; no cap on rows/row length/time → CPU/memory on huge Write/Edit | crates/delilah-agents/src/transcript.rs:432 |
| 3 | security | MEDIUM | transcript_path doesn't validate session_id (`../`) → arbitrary .jsonl read once served over HTTP; validate inside it | crates/delilah-agents/src/transcript.rs:120 |
| 3 | security | LOW | Read summary `o + l` unchecked u64 add (debug panic) → saturating_add | crates/delilah-agents/src/transcript.rs:537 |
| 3 | security | LOW | blocks.clone(), unbounded entries per call, tool_detail unbounded response | crates/delilah-agents/src/transcript.rs:326 |
| 3 | security | LOW | path/tool/ids copied without length cap; path is untrusted, display-only | crates/delilah-agents/src/transcript.rs:422 |
| 3 | architecture | MEDIUM | parse drops tool_result whose tool_use is before the cursor (no orphan results exposed, no two-chunk test) — decide in WP-05/06 | crates/delilah-agents/src/transcript.rs:152 |
| 3 | architecture | MEDIUM | transcript_path is Claude-specific, reads $HOME and canonicalizes; hook path should win; take home as param | crates/delilah-agents/src/transcript.rs:120 |
| 3 | architecture | MEDIUM | transcript.rs is 918 lines mixing parser, API Entry model and diff/summary; Entry carries daemon-only states (Queued/Sent/View) | crates/delilah-agents/src/transcript.rs:1 |
| 3 | architecture | LOW | parse_message uses Utc::now() and t:<msg.id> ids, differing from parse | crates/delilah-agents/src/transcript.rs:201 |
| 3 | architecture | LOW | tool_detail re-parses whole file per call, ignores isSidechain | crates/delilah-agents/src/transcript.rs:227 |
| 3 | architecture | LOW | is_injected is a hard-coded prefix list | crates/delilah-agents/src/transcript.rs:313 |
| 3 | architecture | LOW | rename half done: Live field still `conversation`, set_idea param named conversation | crates/delilah-daemon/src/sessions.rs:48 |
| 3 | architecture | LOW | idea.rs emits Topic::Conversations; keep card↔tool_use dedupe in conversation.rs | crates/delilah-daemon/src/idea.rs |

## Notes for later packages

- WP-04: `parse(bytes, offset)` drops a tool_result whose tool_use is before the cursor → WP-05/06 must re-read from an earlier offset or use `tool_detail` to fill a tool that was pending.
- WP-04: `parse_message` ids are `t:<message.id>:<n>` with `at` = now (no line uuid) — not file-stable.
- WP-04: diff rows use `kind`; ChangesView's TS type uses `type` → WP-08 maps or defines its own type.
- WP-04: no Delilah-specific injected-text marker exists yet; injected prefixes skipped are Claude's own (system-reminder, hooks, command tags).
- WP-02: spike `permission` uses `--permission-prompt-tool stdio` by default; `--prompt-tool none` to test the default.
