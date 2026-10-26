# Progress — 008-group-toasts

Branch: 008-group-toasts · Base: dc8dd49 · Started: 2026-10-26
Mode: format=markdown commits=yes autonomy=auto

| WP | Phase | Group | Status | Attempts | Commit | Notes |
|----|-------|-------|--------|----------|--------|-------|
| WP-01 | 1 | 1A | done | 1 | 493ddbc | run together with WP-02 (one-line setup file) |
| WP-02 | 2 | 2A | done | 1 | 493ddbc | |
| WP-03 | 3 | 3A | verified | 1 | b5bafb1 | parallel with WP-04, WP-07 (disjoint files) |
| WP-04 | 4 | 4A | verified | 1 | 711918b | |
| WP-05 | 5 | 5A | pending | 0 | | |
| WP-06 | 6 | 6A | pending | 0 | | |
| WP-07 | 6 | 6A | verified | 1 | e2183f8 | |
| WP-08 | 7 | 7A | pending | 0 | | |
| WP-09 | 7 | 7A | pending | 0 | | |
| WP-10 | 7 | 7B | pending | 0 | | |

## Open findings

| Phase | Reviewer | Severity | Finding | File |
|-------|----------|----------|---------|------|
| 2 | architecture | MEDIUM | keyed update merges fields (`{...old, ...input}`) so omitted action/onDismiss survive — handed to WP-05 | app/src/components/ui/Toast.tsx |
| 2 | spec-compliance | MEDIUM | evict() can drop the just-pushed info toast when 3 non-info toasts are shown — decided: exclude it from the info-first pass, handed to WP-05 | app/src/components/ui/Toast.tsx |
| 2 | architecture | LOW | clearToasts resets the paused flag (would un-pause a hover at runtime) | app/src/components/ui/Toast.tsx |
| 2 | spec-compliance | LOW | no test for kind "wait" without key never expiring; eviction test no longer asserts exactly 3 remain | app/src/components/ui/Toast.test.tsx |
