# Progress — 004-agent-playground

Branch: feature/LIBRE-3 · Base: 103b0ae · Started: 2026-10-20
Mode: format=markdown commits=yes

Scheduling follows "Depends on" in packages.md: the daemon track (WP-06 → 08 → 09) runs alongside the UI track (WP-01 → 05).

| WP | Phase | Group | Status | Attempts | Commit | Notes |
|----|-------|-------|--------|----------|--------|-------|
| WP-01 | 1 | 1A | done | 1 | 9c190e7 | tsc ok; "Arrancando" already existed |
| WP-02 | 2 | 2A | done | 1 | a7fbadc | 11 tests; tests written with impl (not seen failing); @name of unregistered project gets own area |
| WP-03 | 3 | 3A | done | 2 | 3ff4b70 | 18 tests; fix: empty cells for free slots (no desk shifts); art not yet seen rendered |
| WP-04 | 4 | 4A | pending | 0 | | |
| WP-05 | 5 | 5A | pending | 0 | | |
| WP-06 | 6 | 6A | verified | 2 | da8f944 | 28 tests; tests failed first; ActivityEvent serde tag=type |
| WP-07 | 6 | 6A | pending | 0 | | |
| WP-08 | 6 | 6B | verified | 1 | a32eb33 | 8 tests failed first; mod is private in lib.rs |
| WP-09 | 6 | 6C | verified | 1 | b306827 | 106 daemon tests; red first; hook returns 204 |
| WP-10 | 6 | 6D | pending | 0 | | |
| WP-11 | 6 | 6E | pending | 0 | | |
| WP-12 | 7 | 7A | pending | 0 | | |
| WP-13 | 8 | 8A | pending | 0 | | |
| WP-14 | 9 | 9A | pending | 0 | | |
| WP-15 | 9 | 9B | pending | 0 | | |
| WP-03F | 3 | fix | done | 1 | af94ab7 | arch HIGH --pg-w collision + MEDIUM theme/paths/poses.ts/scale |
| WP-09F | 6 | fix | verified | 1 | (next) | AskUserQuestion, no PreToolUse raw log, helper cap, ghost helpers, WebFetch userinfo |

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
