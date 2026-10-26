# Progress — 008-group-toasts

Branch: 008-group-toasts · Base: dc8dd49 · Started: 2026-10-26
Mode: format=markdown commits=yes autonomy=auto

| WP | Phase | Group | Status | Attempts | Commit | Notes |
|----|-------|-------|--------|----------|--------|-------|
| WP-01 | 1 | 1A | done | 1 | 493ddbc | run together with WP-02 (one-line setup file) |
| WP-02 | 2 | 2A | done | 1 | 493ddbc | |
| WP-03 | 3 | 3A | done | 1 | b5bafb1 | parallel with WP-04, WP-07 (disjoint files) |
| WP-04 | 4 | 4A | done | 1 | 711918b | |
| WP-05 | 5 | 5A | done | 1 | ccd435c | own checks green; 10 tests in other files fail (fallout owned by T022/WP-08); F1+F2 fixed |
| WP-06 | 6 | 6A | done | 1 | 212461a | ProposalCard hover test fails on announcer duplicate text (WP-08) |
| WP-05F | 5 | 5F | done | 1 | 5af8142 | review fixes (1 HIGH, 2 MEDIUM, 1 LOW) |
| WP-05F2 | 5 | 5F | implementing | 0 | | FR-011 «Cerrar todo» while collapsed + stacked look |
| WP-07 | 6 | 6A | done | 1 | e2183f8 | |
| WP-08 | 7 | 7A | implementing | 0 | | |
| WP-09 | 7 | 7A | verified | 1 | a00af6f | decision 79 |
| WP-10 | 7 | 7B | pending | 0 | | |

## Open findings

| Phase | Reviewer | Severity | Finding | File |
|-------|----------|----------|---------|------|
| 2 | architecture | MEDIUM | keyed update merges fields (`{...old, ...input}`) so omitted action/onDismiss survive — fixed in WP-05 | app/src/components/ui/Toast.tsx |
| 2 | spec-compliance | MEDIUM | evict() can drop the just-pushed info toast when 3 non-info toasts are shown — decided: exclude it from the info-first pass, fixed in WP-05 | app/src/components/ui/Toast.tsx |
| 2 | architecture | LOW | clearToasts resets the paused flag (would un-pause a hover at runtime) | app/src/components/ui/Toast.tsx |
| 2 | spec-compliance | LOW | no test for kind "wait" without key never expiring; eviction test no longer asserts exactly 3 remain | app/src/components/ui/Toast.test.tsx |
| 6 | spec-compliance | MEDIUM | ProposalCard unmount flush sends «Descartar» but leaves the «Tarea descartada» toast — handed to WP-08 | app/src/components/ProposalCard.tsx |
| 3 | spec-compliance | LOW | scenario 3 test doesn't check the action goes back to «Responder» | app/src/components/GlobalNotices.test.tsx |
| 4 | spec-compliance | LOW | merge restarts with the incoming ttl (default 6 s), not the merged toast's own ttl; incoming onDismiss dropped | app/src/components/ui/Toast.tsx |
| 4 | spec-compliance | LOW | no test that a keyed toast isn't merged | app/src/components/ui/Toast.test.tsx |
| 5 | architecture | HIGH | announcer mounted only while toasts exist (first toast may not be announced) — fixed in WP-05F | app/src/components/ui/Toast.tsx |
| 5 | architecture | MEDIUM | hovered/focused/pinned not reset when host empties (next toast opens expanded) — fixed in WP-05F | app/src/components/ui/Toast.tsx |
| 5 | architecture | MEDIUM | «+N» unmounts on activation (focus lost), no toggle, aria-expanded hardcoded — fixed in WP-05F | app/src/components/ui/Toast.tsx |
| 5 | architecture | LOW | identical consecutive announcements not re-announced (lastAnnounced plain string) — fixed in WP-05F | app/src/components/ui/Toast.tsx |
| 5 | architecture | LOW | ToastHost interaction state grows next to the store; consider splitting store/view later | app/src/components/ui/Toast.tsx |
| 5 | spec-compliance | MEDIUM | «Cerrar todo» only rendered while expanded (FR-011: whenever >1) — fix round 2 WP-05F2 | app/src/components/ui/Toast.tsx |
| 5 | spec-compliance | LOW | no stacked/behind look for hidden toasts (T016) — WP-05F2 | app/src/components/ui/Toast.css |
| 5 | spec-compliance | LOW | long-text clamp is CSS-only, untested | app/src/components/ui/Toast.css |
