# Progress — 004-project-config

Branch: feature/LIBRE-2 · Base: d13d825 · Started: 2026-10-21
Mode: format=markdown commits=yes

| WP | Phase | Group | Status | Attempts | Commit | Notes |
|----|-------|-------|--------|----------|--------|-------|
| WP-01 | 1 | 1A | done | 1 | d2ef468 | |
| WP-02 | 2 | 2A | done | 2 | 3aea005 | +core Cargo.toml dev-dep; FieldError has code |
| WP-03 | 2 | 2A | done | 1 | d58e628 | |
| WP-04 | 2 | 2B | done | 1 | 6ec349e | |
| WP-05 | 2 | 2A | done | 1 | b3fe227 | |
| WP-06 | 2 | 2A | done | 1 | 41936c8 | +Sidebar.test.tsx fixture |
| WP-07 | 3 | 3A | pending | 0 | | |
| WP-08 | 3 | 3A | pending | 0 | | |
| WP-09 | 3 | 3A | pending | 0 | | |
| WP-10 | 4 | 4A | pending | 0 | | |
| WP-11 | 4 | 4B | pending | 0 | | |
| WP-12 | 4 | 4B | pending | 0 | | |
| WP-13 | 5 | 5A | pending | 0 | | |
| WP-14 | 5 | 5A | pending | 0 | | |
| WP-15 | 5 | 5B | pending | 0 | | |
| WP-16 | 5 | 5C | pending | 0 | | |
| WP-17 | 5 | 5D | pending | 0 | | |
| WP-18 | 5 | 5D | pending | 0 | | |
| WP-19 | 6 | 6A | pending | 0 | | |
| WP-20 | 6 | 6A | pending | 0 | | |
| WP-21 | 7 | 7A | pending | 0 | | |
| WP-22 | 7 | 7A | pending | 0 | | |
| WP-23 | 8 | 8A | pending | 0 | | |
| WP-24 | 8 | 8B | pending | 0 | | |
| WP-02F | 2 | 2F | done | 1 | dfcf408 | review fixes: pty exit outcome, FieldError.code in TS + api.md |

## Review notes

Phases 1–2 (d13d825..53c4cb0): spec ✅ compliant · contract ⚠️ needs care · migration ✅ safe · architecture ✅ approve.
Carried into later briefs:
- WP-07: unknown-key PUT body must return 422 `{error, fields}`; PATCH changing kind must re-validate or clear submodule rules; trim blank `setup` to null on save.
- WP-15: `launch_agent` returns a domain error (not ApiError) and is `pub(crate)`; AgentLaunch prompt as an enum if convenient.
Open LOW (not fixed): migration test on pre-existing rows; SharedMode→ShareMode TS rename; resolved_mode helper; shared Rust/TS fixture test; key.to_string() repeats.
