# Progress — 004-agent-playground

Branch: feature/LIBRE-3 · Base: 103b0ae · Started: 2026-10-20
Mode: format=markdown commits=yes

Scheduling follows "Depends on" in packages.md: the daemon track (WP-06 → 08 → 09) runs alongside the UI track (WP-01 → 05).

| WP | Phase | Group | Status | Attempts | Commit | Notes |
|----|-------|-------|--------|----------|--------|-------|
| WP-01 | 1 | 1A | done | 1 | 9c190e7 | tsc ok; "Arrancando" already existed |
| WP-02 | 2 | 2A | done | 1 | a7fbadc | 11 tests; tests written with impl (not seen failing); @name of unregistered project gets own area |
| WP-03 | 3 | 3A | done | 2 | 3ff4b70 | 18 tests; fix: empty cells for free slots (no desk shifts); art not yet seen rendered |
| WP-04 | 4 | 4A | done | 1 | 84c2379 | class pg-desk--waiting; counter always shown (muted at 0) | |
| WP-05 | 5 | 5A | done | 1 | 8eab1c9 | native window not tried (needs desktop app); src-tauri test needed a 0-byte sidecar stub (gitignored) |
| WP-06 | 6 | 6A | done | 2 | da8f944 | 28 tests; tests failed first; ActivityEvent serde tag=type |
| WP-07 | 6 | 6A | done | 1 | ff47dce | |
| WP-08 | 6 | 6B | done | 1 | a32eb33 | 8 tests failed first; mod is private in lib.rs |
| WP-09 | 6 | 6C | done | 1 | b306827 | 106 daemon tests; red first; hook returns 204 |
| WP-10 | 6 | 6D | done | 1 | 69f6767 | App fetchers no-op activity; new state words need en.ts entries (WP-11) |
| WP-11 | 6 | 6E | done | 1 | 329656f | data-pose is now the pose name; en.ts gets Editing/Reading/Running |
| WP-12 | 7 | 7A | done | 1 | dce6ced | 8 tests red first; new palette key y; spots in module-level map |
| WP-13 | 8 | 8A | done | 1 | e29ddfd | T044 (art pass with screenshots) left to the user: no browser in this session |
| WP-14 | 9 | 9A | done | 2 | 40d4492 | first run cut by usage limit |
| WP-15 | 9 | 9B | done | 1 | (this) | all checks green (workspace, src-tauri, UI 111); quickstart in a browser left to the user |
| WP-03F | 3 | fix | done | 1 | af94ab7 | arch HIGH --pg-w collision + MEDIUM theme/paths/poses.ts/scale |
| WP-09F | 6 | fix | done | 1 | 15d97b9 | AskUserQuestion, no PreToolUse raw log, helper cap, ghost helpers, WebFetch userinfo |
| WP-05F | 5 | fix | done | 1 | 8c08ade | hide-on-close only for main; payload validation; in-flight guard |
| WP-12F | 7 | fix | done | 2 | 3f535c4 | leaving helpers never removed (HIGH); visible helper label (MEDIUM); reserve leaving spot |

## Reviews

- Phases 1–2 (103b0ae..a7fbadc): spec-compliance COMPLIANT (LOW only); architecture no CRITICAL/HIGH.
  - MEDIUM slot stability across unmount → folded into WP-03 (slots kept in usePlayground).
  - MEDIUM `activity` topic forcing a fetcher in App's Record<Topic,…> → handled in WP-10.
  - MEDIUM duplicated topic→fetch loader (App vs usePlayground) → deferred: shared `useTopics` refactor later.
  - LOW: no nav shortcut for Playground; "—" used as key and label; no fallback poll offline.

- Phase 3 (3ff4b70): spec COMPLIANT (LOW only); architecture HIGH `--pg-w` colour/size collision → WP-03F (with MEDIUMs: token-based palette, path-per-colour + memo, poses.ts, --pg-scale).
- Daemon track (da8f944, a32eb33, b306827): contract and security no CRITICAL/HIGH → WP-09F for the MEDIUMs/LOWs.
  - Note for WP-10: add "activity" to live.ts ALL as well as Topic; Action.target is string|null; Helper.role may be "".
  - Pre-existing, out of scope: hooks.jsonl has no size cap/rotation; PostToolUse already logs tool_response.
  - Visual check of the office (ui-verifier in a preview) deferred until WP-05 lands.
- Phase 4 (84c2379): spec COMPLIANT; LOW: click test fixture has owner = session id; tasks.md still says .desk--waiting (class is pg-desk--waiting).
- Phase 5 (8eab1c9, security+contract+spec in one pass): HIGH hidden playground not re-shown → WP-05F; MEDIUM payload validation → WP-05F; LOW separate capability for playground window: not done (same bundle/CSP).
- ui-verifier: no browser tools in this session — visual scenarios not checked; preview 1 left running for the user (http://localhost:1501).
- US3 UI + US4 (69f6767, 329656f, ff47dce, dce6ced): US3 COMPLIANT; US4 GAPS → WP-12F. LOW not fixed: capitalised state word in accessible names; SPOTS entry kept if a desk unmounts with helpers.
