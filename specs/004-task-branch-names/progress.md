# Progress — 004-task-branch-names

Branch: feature/LIBRE-6 · Base: c04ebc2 · Started: 2026-10-22
Mode: format=markdown commits=yes

| WP | Phase | Group | Status | Attempts | Commit | Notes |
|----|-------|-------|--------|----------|--------|-------|
| WP-01 | 1 | 1A | done | 2 | | redone after a parallel checkout reverted it; +1 line in 17 test Config literals |
| WP-02 | 2 | 2A | done | 1 | | 22 tests, clippy clean |
| WP-03 | 2 | 2A | done | 1 | | 28 tests, clippy clean |
| WP-04 | 2 | 2A | done | 1 | | clippy pending (core in progress) |
| WP-05 | 3 | 3A | verified | 1 | | |
| WP-06 | 4 | 4A | verified | 1 | | |
| WP-07 | 4 | 4A | verified | 1 | | run early (UI only, api mocked) |
| WP-08 | 5 | 5A | verified | 1 | | run early alongside WP-05 (no shared files) |
| WP-09 | 5 | 5A | verified | 1 | | |
| WP-10 | 6 | 6A | verified | 1 | | decision 33 |
| WP-11 | 6 | 6B | pending | 0 | | |
| WP-02F | 2 | fix | done | 1 | | review fixes: disableAllHooks, shared one-shot helper, claude_bin in suggest, key slug, compose guard, decomposed accents, remotes with `/` |

## Reviews

- Phases 1–2: spec COMPLIANT; security no HIGH (MEDIUM: user hooks load on the naming call); architecture no HIGH (MEDIUM: envelope/args/env duplicated with suggest.rs, suggest ignores claude_bin). Skipped LOW: Config::for_tests constructor, pure next_free in core.
| WP-05F | 3–5 | fix | implementing | 0 | | backend: keep the key's case in sent names, normalise Claude's answer like a typed one, for_start helper, safe submodule dirs |
| WP-07F | 3–5 | fix | done | 1 | | UI: no trim per keystroke, notes refresh the proposal |

- Phases 3–5: spec GAPS (MEDIUM: Rama field trims each keystroke); security no HIGH/MEDIUM (LOW: submodule paths unvalidated in dirs_of); architecture HIGH (dialog-sent Jira names lowercased by `custom`), MEDIUM (double Claude call if the UI omitted the name — resolved by keeping the UI's name and preserving the key). Skipped LOW: move the handler out of branch_name.rs; TOCTOU between `unique` and workspace creation (creation fails cleanly).
