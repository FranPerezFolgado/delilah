# Progress — 003-pr-loop

Branch: feature/LIBRE-1 · Base: 0b2eebf · Started: 2026-10-20
Mode: format=markdown commits=yes

Baseline (T001): workspace and app tests green; `app/src-tauri` tests can't build here (sidecar `binaries/delilahd-*` not built in this worktree; pre-existing, untouched by this feature).

| WP | Phase | Group | Status | Attempts | Commit | Notes |
|----|-------|-------|--------|----------|--------|-------|
| WP-01 | 1 | 1A | done | 1 | 6ba6e64 | review: compliant; migration safe (LOW only) |
| WP-02 | 2 | 2A | done | 1 | 4cf0786 | |
| WP-03 | 3 | 3A | done | 1 | 0c88a0b | |
| WP-04 | 3 | 3B | done | 1 | a35b58e | |
| WP-05 | 3 | 3A | done | 1 | e2d3260 | |
| WP-06 | 4 | 4A | done | 1 | 0a0db35 | |
| WP-07 | 4 | 4A | done | 1 | e9b7c18 | |
| WP-08 | 4 | 4A | done | 1 | 9d2c80e | |
| WP-09 | 4 | 4A | done | 1 | 52a69d3 | |
| WP-10 | 5 | 5A | done | 1 | a436bd0 | |
| WP-11 | 5 | 5B | done | 1 | 4084ebd | |
| WP-12 | 5 | 5A | done | 1 | cf6294f | |
| WP-13 | 6 | 6A | done | 1 | 1ecd21d | |
| WP-14 | 6 | 6A | done | 1 | d12155b | |
| WP-15 | 6 | 6B | done | 1 | 3269020 | |
| WP-16 | 6 | 6A | done | 1 | f94bd8d | |
| WP-17 | 7 | 7A | done | 1 | 3269020 | merged into WP-15 |
| WP-18 | 7 | 7A | done | 1 | 4084ebd | merged into WP-11 |
| WP-19 | 7 | 7A | done | 1 | 3e14945 | |
| WP-20 | 8 | 8A | done | 1 | b19bbc5 | |
| WP-21 | 8 | 8B | done | 1 | 0a788f2 | |
| WP-02F | 2 | 2F | done | 1 | 74f6081 | phase 2 review fixes (arch MEDIUM ×3, security LOW ×2, spec LOW) |
| WP-03F | 3 | 3F | done | 1 | b424c58 | phase 3 review fixes (arch HIGH, spec GAP FR-022, security MEDIUM ×2, arch MEDIUM/LOW) |
| WP-04F | 4 | 4F | done | 1 | b714bf4 | phase 4 review fixes (spec HIGH FR-012, MEDIUM FR-032, security MEDIUM ×2, contract ×2, flaky test) |
| WP-07F | 7 | 7F | done | 2 | d5cef76 deeb5d2 | split into F1 (merge safety, archive module, inbox, notifications) and F2 (proposal, link_known, pr_api split) after a usage-limit stop |
| WP-07G | 7 | 7F | done | 1 | a0313f7 | phases 5–7 review fixes, app (open button, archive copy, merge alert) |
| WP-20F | 8 | 8F | done | 1 | d9ad304 | docs vs code after 7F (merge checks every repo, Abrir PR while a repo is free, one notification, archive module) |
