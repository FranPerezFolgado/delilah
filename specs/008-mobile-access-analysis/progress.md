# Progress — 008-mobile-access-analysis

Branch: 008-mobile-access-analysis · Base: 7db6a17 · Started: 2026-10-26
Mode: format=markdown commits=yes autonomy=auto

Experiment (WP-03): run with the user on 2026-10-26.

| WP | Phase | Group | Status | Attempts | Commit | Notes |
|----|-------|-------|--------|----------|--------|-------|
| WP-01 | 1 | 1A | done | 1 | b52d62c | first run hung, relaunched |
| WP-02 | 2 | 2A | done | 1 | 492c9c5 | |
| WP-03 | 3 | 3A | done | 1 | f520f5e | run with the user 2026-10-26 |
| WP-04 | 3 | 3B | done | 1 | 7bceddb | also fixed phase 1–2 findings |
| WP-05 | 4 | 4A | done | 2 | cc46b93 | first run hung, relaunched |
| WP-06 | 4 | 4A | done | 1 | b37eacf | renumbered to decision 87 after merging main (c41954e) |
| WP-07 | 4 | 4A | done | 1 | ac2805e | |
| WP-08 | 4 | 4B | done | 1 | 65a2ee9 | links ok; architecture.md/README unchanged |
| WP-09 | 5 | 5A | done | 1 | df40e7c | bump 1.1.0 → 1.1.1 (patch) |

## Open findings

| Phase | Reviewer | Severity | Finding | File |
|-------|----------|----------|---------|------|
| 1–2 | spec-compliance | MEDIUM | R1 doesn't separate "browser tab" vs "installed home-screen app" as options (FR-001/US1) | research.md:22-34 |
| 1–2 | spec-compliance | MEDIUM | Push/background cells for B–E, sideload 7-day/3-app and EU claims lack sources (SC-002) | research.md:24,81-87 |
| 1–2 | spec-compliance | LOW | No cost row; Cloudflare needs a domain (usually paid) | research.md:24,29 |
| 1–2 | spec-compliance | LOW | "Store review" not stated per option (US1 sc.1) | research.md:24 |
| 1–2 | spec-compliance | LOW | Edge case "daemon down, Mac awake" not covered | research.md:30 |
| 1–2 | spec-compliance | LOW | SC-005: not said one-time; pairing not counted in setup time | research.md:29 |
| 1–2 | spec-compliance | LOW | Missing blank line after R5 table breaks rendering | research.md:185 |

Phase 1–2 findings: fixed in WP-04 (7bceddb). Phase 3 findings (3 MEDIUM, LOWs): fixed in WP-04F (56d5003).
Open: SC-004 not verifiable before push (step 4); Q6 not run; iOS version not recorded.
