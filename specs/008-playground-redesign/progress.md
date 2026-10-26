# Progress — 008-playground-redesign

Branch: feature/LIBRE-3 · Base: 306e335 · Started: 2026-10-26
Mode: format=markdown commits=yes autonomy=auto
Scheduling: by "Depends on" in packages.md (early start approved for WP-09/WP-12; WP-14/WP-15 also start as soon as WP-06 is done).

| WP | Phase | Group | Status | Attempts | Commit | Notes |
|----|-------|-------|--------|----------|--------|-------|
| WP-01 | 1 | 1A | done | 1 | 8d5d7dc |  |
| WP-02 | 2 | 2A | done | 1 | aa74fdf |  |
| WP-03 | 2 | 2A | done | 1 | 1191bb8 |  |
| WP-06 | 2 | 2A | done | 1 | 6eca0a4 |  |
| WP-04 | 2 | 2B | done | 1 | 24269c2 | needs 02,03 |
| WP-07 | 2 | 2B | done | 1 | 730e028 | needs 01 |
| WP-08 | 2 | 2C | done | 1 | 0376212 | needs 01,07 |
| WP-05 | 2 | 2C | done | 1 | 41827dc | needs 02,04 |
| WP-09 | 3 | 2B | verified | 1 | b03fa7f | early: needs 06 |
| WP-10 | 3 | 3B | verified | 1 | 0ebdb82 |  |
| WP-11 | 3 | 3C | verified | 1 | 30a0d03 |  |
| WP-12 | 4 | 3C | verified | 1 | 31a41fa | early: with 11 |
| WP-13 | 4 | 4B | implementing | 1 | |  |
| WP-14 | 5 | 2B | verified | 1 | 5bbdcd9 | early: needs 06 |
| WP-15 | 5 | 2B | verified | 1 | 5533380 | early: needs 06 |
| WP-16 | 5 | 5B | verified | 1 | d2c88da |  |
| WP-17 | 5 | 5C | pending | 0 | | also fix Timeline.css --acc → --focus/--wait |
| WP-18 | 6 | 6A | pending | 0 | |  |
| WP-19 | 6 | 6A | pending | 0 | |  |
| WP-05F | 2 | fix | verified | 1 | PENDING | phase 2 review fixes |
| WP-10F | 3 | fix | verified | 1 | 27fbd7d | styles.test scale/token fixes |

## Open findings

| Phase | Reviewer | Severity | Finding | File |
|-------|----------|----------|---------|------|
| 2 | contract | LOW | history for unknown owner also returns started_at (doc says only sessions) | contracts/api.md |
| 2 | contract | LOW | axum Query rejects malformed query strings with plain-text 400, not ApiError JSON | session_api.rs |
| 2 | contract | LOW | seq/started_at reset on daemon restart; client relies on reconnect full refetch | useActivityHistory.ts |
| 2 | architecture | LOW | sessions.all() snapshot per hook event; a Sessions::live_ids() helper would dedupe 3 copies | hooks.rs, session_api.rs |
| 2 | architecture | LOW | PixelSprite keyframes only up to 16 frames; module cache unbounded (small key space) | PixelSprite.tsx |
| 2 | architecture | LOW | ROLE_LOOKS keyed by hardcoded Spanish role names | art/outfit.ts |
| 2 | spec | LOW | research R2 says emit on every append; implementation coalesces via dirty flag (tasks.md) | research.md |
| 2 | spec | LOW | PixelSprite reduced-motion / .pg-paused only in CSS, untested | PixelSprite.css |
