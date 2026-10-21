# Progress — 004-saving-mode

Branch: feature/LIBRE-5 · Base: 6cea6a2 · Started: 2026-10-21
Mode: format=markdown commits=yes

| WP | Phase | Group | Status | Attempts | Commit | Notes |
|----|-------|-------|--------|----------|--------|-------|
| WP-01 | 2 | 2A | done | 1 | 27b27b6 | +hooks.rs (step: None in test literal) |
| WP-01F | 2 | 2A | done | 1 | 6826120 | phase 2 review fixes F1–F6 |
| WP-02 | 3 | 3A | verified | 1 | b83caae | |
| WP-03 | 3 | 3B | verified | 1 | 82ee202 | +backlog saving_mode (create/PATCH) |
| WP-04 | 3 | 3B | verified | 1 | ad4894d | |
| WP-05 | 3 | 3B | verified | 1 | | +TaskView.tsx label, alerts.ts, 4 test fixtures |
| WP-06 | 4 | 4A | pending | 0 | | |
| WP-07 | 5 | 5A | pending | 0 | | |
| WP-08 | 6 | 6A | pending | 0 | | |
| WP-09 | 7 | 7A | pending | 0 | | |
| WP-10 | 7 | 7B | pending | 0 | | |

## Reviews

- Phase 2: spec GAPS→fixed, migration NEEDS CARE→fixed, architecture approve-with-changes→fixed, contract NEEDS CARE (no break). Fixed in WP-01F.
  - Open (LOW): bad `handoff` JSON fails the whole task list (store.rs task_from_row); no test of 0011 over pre-existing rows; no shared insert_event helper (hand_over writes task_events directly).
  - For WP-05: api.ts Task gets `saving_mode`, `handoff`; Session gets `step`; EVENT_LABELS gets "step-started"; tolerate inbox kind "handoff". CLI must ship with the daemon (strict Event enum).
