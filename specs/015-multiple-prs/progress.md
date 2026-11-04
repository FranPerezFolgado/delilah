# Progress — 015-multiple-prs

Branch: 015-multiple-prs · Base: 04519fd · Started: 2026-11-03
Mode: format=markdown commits=no autonomy=auto

| WP | Phase | Group | Status | Attempts | Commit | Notes |
|----|-------|-------|--------|----------|--------|-------|
| WP-01 | 1 | 1A | done | 1 | | |
| WP-02 | 2 | 2A | done | 1 | | |
| WP-03 | 2 | 2A | done | 2 | | |
| WP-04 | 2 | 2B | done | 1 | | |
| WP-05 | 2 | 2B | done | 2 | | |
| WP-06 | 2 | 2C | done | 1 | | |
| WP-07 | 2 | 2D | done | 1 | | |
| WP-08 | 3 | 3A | done | 1 | | |
| WP-09 | 3 | 3A | done | 1 | | |
| WP-10 | 3 | 3B | done | 1 | | |
| WP-11 | 3 | 3C | done | 1 | | |
| WP-12 | 4 | 4A | done | 1 | | |
| WP-13 | 4 | 4A | done | 1 | | |
| WP-14 | 4 | 4B | done | 1 | | |
| WP-15 | 5 | 5A | done | 1 | | |
| WP-16 | 5 | 5B | done | 1 | | |
| WP-17 | 5 | 5A | done | 1 | | |
| WP-18 | 5 | 5A | done | 2 | | |
| WP-19 | 6 | 6A | done | 1 | | |
| WP-20 | 6 | 6B | done | 0 | | 20a, 20F, 20G, T048 (1.4.1→1.4.2, uncommitted) done; quickstart preview walk + Tauri tests left to user |

## Open findings

| Phase | Reviewer | Severity | Finding | File |
|-------|----------|----------|---------|------|
| 2 | migration | MEDIUM | 0027 is one-way (no down migration); note in docs/decisions.md that a DB backup is needed before upgrading | migrations/0027_pr_per_url.sql |
| 2 | contract | MEDIUM | InboxView.tsx:502 retries by repo only; with two PRs in one repo it hits the first counted PR (inbox item exposes pr_url, unused by app) | app/src/components/InboxView.tsx |
| 2 | architecture | MEDIUM | Store::insert_notice silently writes pr_url='' for any kind (PR-kind notices unmatchable) | crates/delilah-daemon/src/store_prs.rs |
| 2 | architecture | MEDIUM | 0027 doesn't backfill pr_url for autofix-stopped notices; resolve_stopped keeps a legacy repo-wide fallback | migrations/0027_pr_per_url.sql |
| 2 | architecture | MEDIUM | prs::summary clones rows just to filter; document whether list shows replaced rows | crates/delilah-daemon/src/prs.rs |
| 3 | (self) | LOW | capture: chained commands like 'cat f; gh pr create' mix outputs (only whole https PR URLs, max 4, deduped) | pr_loop/capture.rs |
| 4 | (self) | LOW | unused `LinkError` import in pr_loop/capture.rs (clippy) — fix in final cleanup | crates/delilah-daemon/src/pr_loop/capture.rs |
| 4 | (self) | LOW | autopilot/finish.rs:457 still checks "repo has a PR" by repo only; open-PR head check applies only to `.` of a parent (FakeGh head fixed to feat/x) | autopilot/finish.rs, pr_api/open.rs |
| final | security | LOW | agent-printed URL can name any host; daemon runs `gh api graphql --hostname <host>` (blind request; gh sends a token only for hosts it is signed in to). Optionally limit to known hosts | pr_loop/capture.rs |
| final | security | LOW | `gh pr create` command check is a heuristic (misses `bash -c "gh pr create"`); only costs a missed link | pr_loop/capture.rs |
| final | contract | LOW | mixed deploy (old client + new daemon) accepted | api.rs / api.ts |
| final | contract | LOW | retry_pr accepts a replaced PR | pr_api/mod.rs |
| final | architecture | HIGH | pre_archive_detect fail-open: a gh error in detect() is ignored and the archive still runs (FIXED by WP-20F, arch re-review OK) | pr_loop/mod.rs |
| final | architecture | HIGH | primary_working read once before multi-second forced detection; archive runs on stale rows (FIXED by WP-20F, arch re-review OK) | pr_loop/mod.rs |
| final | architecture | MEDIUM | capture vs detection race on same PR: link_pr_on_branch ON CONFLICT overwrites repo/linked_by; repo_of falls back to host label on transient repo_view failure | store_prs.rs, pr_loop/capture.rs |
| final | architecture | MEDIUM | Blocked archive re-runs forced detection on every 30 s poll | pr_loop/mod.rs |
| final | architecture | MEDIUM | primary session stuck Working (held stop, shell/setup session) blocks archiving silently | pr_loop/mod.rs |
| final | architecture | LOW | turn-end nudge does prs_of query on every TurnEnded; repo_of mixes three resolution strategies, serial repo_view | pr_loop/ |
| final | spec-compliance | HIGH | FR-009: detect() uses plain link_pr, so a closed-unmerged PR is not marked replaced when detection finds the new PR on the same repo+branch (FIXED by WP-20F, arch re-review OK) | pr_loop/mod.rs:368, store_prs.rs |
| final | spec-compliance | HIGH | FR-013: parent repo `.` is never safety-checked (verify/check_merged loop over ws.repos() only) yet its branch is deleted on archive; unpushed commits could be lost (FIXED by WP-20G; spec re-review COMPLIANT) | pr_loop/merge.rs, delilah-git/workspace.rs, archive.rs |
| final | spec-compliance | LOW | archive.rs:126 adds `.` to branches to delete whenever any repo verified, even if parent had no PR (pre-existing; check intent) | archive.rs |
| final | spec-compliance | LOW | no end-to-end test runs archive deletion of "." through verify() in merge.rs (covered piecewise) | pr_loop/merge.rs |
| final | architecture | LOW | a turn could start between the last primary_working check and on_merged (pre-existing window) | pr_loop/mod.rs:262 |
