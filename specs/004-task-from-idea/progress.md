# Progress — 004-task-from-idea

Branch: feature/LIBRE-4 · Base: 3461a74 · Started: 2026-10-21
Mode: format=markdown commits=yes

| WP | Phase | Group | Status | Attempts | Commit | Notes |
|----|-------|-------|--------|----------|--------|-------|
| WP-01 | 1 | 1A | done | 1 | 924520d | spike: (a)-(f) pass |
| WP-02 | 1 | 1A | done | 1 | c05ac7c | |
| WP-03 | 2 | 2A | done | 1 | e460b09 | allow list also allows `Bash(rtk <cmd>:*)` forms (user decision); record in docs/decisions.md |
| WP-04 | 2 | 2A | done | 1 | faac645 | wire type "created-from-conversation" (kebab); JSON round-trip test moved to WP-05 |
| WP-07 | 2 | 2A | done | 1 | 5a88680 | App.tsx: no-op conversations fetcher |
| WP-05 | 2 | 2B | done | 1 | 7dae09c | +JSON round-trip of CreatedFromConversation |
| WP-06 | 2 | 2B | done | 1 | 2b0eda4 | |
| WP-08 | 2 | 2C | done | 2 | 4f7a642 | |
| WP-02F | 2 | 2F | done | 1 | 050dadb | phase 2 review fixes |
| WP-09 | 3 | 3A | done | 1 | b1f19f6 | |
| WP-11 | 3 | 3A | done | 1 | 852b678 | new strings for WP-18: Creada, Crear, Cambiar algo, Abrir terminal, Escribe un mensaje…, Mensaje, Quitar mensaje, en cola, Cuéntale tu idea…, Claude espera en la terminal |
| WP-10 | 3 | 3B | done | 1 | 9e98f38 | gap: card stays pending if hook connection drops |
| WP-12 | 3 | 3C | done | 1 | 502d1e6 | strings for WP-18: Ver terminal, Ver conversación, Nueva tarea con Claude (en {name}), Con Claude, Tu idea, Opcional: cuéntale…, Abrir conversación |
| WP-03Fd | 3 | 3F | done | 1 | 87c606d | phase 3 fixes, daemon; stop on unknown session now 404 |
| WP-03Fu | 3 | 3F | done | 1 | d9447f0 | phase 3 fixes, UI; strings for WP-18: Respondida en la terminal, Retirada |
| WP-13 | 4 | 4A | verified | 1 | 551037a | |
| WP-14 | 4 | 4A | verified | 1 | 46f9f9f | |
| WP-04F | 4 | 4F | done | 1 | | phase 4 fixes: hermetic run test |
| WP-15 | 5 | 5A | pending | 0 | | |
| WP-16 | 5 | 5A | verified | 1 | 12d54c8 | InboxView change untested | |
| WP-17 | 6 | 6A | pending | 0 | | |
| WP-18 | 7 | 7A | pending | 0 | | |
| WP-19 | 7 | 7A | pending | 0 | | |
| WP-20 | 7 | 7B | pending | 0 | | T041 manual walkthrough |

## Reviews

- Phase 1: spec-compliance COMPLIANT. MEDIUM (RTK rewrites break read-only Bash allow list) → decided: also allow `rtk` forms, done in WP-03.
- Phase 2: spec COMPLIANT (minor gaps) · migration SAFE · contract 1 HIGH (Entry not flattened) · security 2 HIGH (rg --pre exec, git --output write), 1 MEDIUM (repo settings merged). Fixed in WP-02F: allow list cut to git status/ls (+rtk), MultiEdit denied, --setting-sources user for dontAsk, Entry flattened, allow-edits refuses conversations, prompt recorded as first user entry, label forced "Idea", docs aligned. LOW kept: plugin dir stays on all project sessions (FR-014 allows them to create tasks).
- Phase 3: spec GAPS (HIGH pending cards not interactive, MEDIUM duplicate ask-bar) · contract BREAKING (same CRITICAL; card types; dead card after failed Crear) · security no HIGH (MEDIUM control chars to PTY; LOW stop on any session, owner not checked vs record, no size caps, send race) · architecture 2 HIGH (asks↔conversation cycle; proposal logic in ask handler) · UI verifier: no browser tools, nothing checked. Fixed in WP-03Fd/WP-03Fu. Deferred: usage-limit notice in the conversation view (no task covers it), TaskScreen SessionTab split, build_task move to a service module; "Otra" not offered on proposal cards (scenario 5 lists only the three actions).
- Phase 4: spec GAPS — HIGH run test launches the real claude (fails without it), MEDIUM tautological prompt assertion, LOW temp dir left behind → WP-04F. Deferred LOW: the history label isn't visible in BacklogTaskView (no history list there; the notes mark shows the origin).
