# Progress — 004-saving-mode

Branch: feature/LIBRE-5 · Base: 6cea6a2 · Started: 2026-10-21
Mode: format=markdown commits=yes

| WP | Phase | Group | Status | Attempts | Commit | Notes |
|----|-------|-------|--------|----------|--------|-------|
| WP-01 | 2 | 2A | done | 1 | 27b27b6 | +hooks.rs (step: None in test literal) |
| WP-01F | 2 | 2A | done | 1 | 6826120 | phase 2 review fixes F1–F6 |
| WP-02 | 3 | 3A | done | 1 | b83caae | |
| WP-03 | 3 | 3B | done | 1 | 82ee202 | +backlog saving_mode (create/PATCH) |
| WP-04 | 3 | 3B | done | 1 | ad4894d | |
| WP-05 | 3 | 3B | done | 1 | 7ed3968 | +TaskView.tsx label, alerts.ts, 4 test fixtures |
| WP-03F | 3 | 3B | done | 1 | daba6f5 | phase 3 review fixes (daemon, CLI); F6 skipped (Live.primary readers out of scope) |
| WP-05F | 3 | 3B | done | 1 | f2f90b6 | phase 3 review fixes (history label) |
| WP-06 | 4 | 4A | done | 1 | 7d176ae | +handoff offer buttons on task screen; running saving-mode toggle (phase 3 spec review) |
| WP-06F | 4 | 4A | done | 1 | 6855266 | phase 4 review fixes |
| WP-07 | 5 | 5A | done | 1 | 9ff12ac | |
| WP-08 | 6 | 6A | done | 1 | 5a98a51 | +phase 5 LOWs: write Saving before typing; tests for typed text and limited signals while saving |
| WP-08F | 6 | 6A | done | 1 | d8d7411 | phase 6 review fixes |
| WP-09 | 7 | 7A | done | 1 | 2a984e9 | |
| WP-10 | 7 | 7B | skipped | 1 | | checks: workspace + web green; tauri needs binaries/delilahd sidecar (env); quickstart walk left to user (needs real agents) |

## Reviews

- Phase 2: spec GAPS→fixed, migration NEEDS CARE→fixed, architecture approve-with-changes→fixed, contract NEEDS CARE (no break). Fixed in WP-01F.
  - Open (LOW): bad `handoff` JSON fails the whole task list (store.rs task_from_row); no test of 0011 over pre-existing rows; no shared insert_event helper (hand_over writes task_events directly).
  - For WP-05: api.ts Task gets `saving_mode`, `handoff`; Session gets `step`; EVENT_LABELS gets "step-started"; tolerate inbox kind "handoff". CLI must ship with the daemon (strict Event enum).
- Phase 3: spec GAPS→fixed/deferred, architecture approve-with-changes→fixed, contract NEEDS CARE (FR-002 risk was a false alarm), security OK (2 LOW, 1 fixed), UI PARTIAL (no browser; API checks pass, 81 web tests). Fixed in WP-03F + WP-05F.
  - Moved to WP-06: handoff offer buttons on the task screen (US1-1/FR-005); running saving-mode toggle (FR-001).
  - Open (LOW): old Live keeps primary=true until it exits (~3 s) — F6 skipped, Live.primary readers span asks/actions/hooks/terminal; review handoff starts silently until WP-08 (notification); English history says "Moved to Implement" (reuses phase-name keys); module split of handoff.rs handlers; PATCH not atomic; no Origin check verified for state-changing POSTs (pre-existing).
- Phase 4: spec COMPLIANT, architecture approve-with-minor-changes → fixed in WP-06F (shared stepLabel in chain.ts, owner-aware primary-follow effect, typed stages, tests). UI not checked visually (no browser tools in this session).
- Phase 5: spec COMPLIANT (LOW test gaps + Saving written after typing → folded into WP-08). No other reviewers named.
- Phase 6: spec COMPLIANT, contract NEEDS CARE (duplicate/misworded alert, stale contract doc) → fixed in WP-08F (announce quiet when app listens, neutral alert wording, starting text on task screen, 2 tests, contract doc).
- Phase 7: docs written (decision 26); full checks: cargo workspace OK (daemon 149), web 105 OK + tsc clean; app/src-tauri test can't build here (missing binaries/delilahd-aarch64-apple-darwin, produced by the release build; branch doesn't touch src-tauri). T036 left unchecked: quickstart not walked.
