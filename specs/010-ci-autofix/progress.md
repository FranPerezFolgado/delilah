# Progress — 010-ci-autofix

Branch: 010-ci-autofix · Base: 8d6c284 · Started: 2026-10-26
Mode: format=markdown commits=yes autonomy=auto

| WP | Phase | Group | Status | Attempts | Commit | Notes |
|----|-------|-------|--------|----------|--------|-------|
| WP-01 | 1 | 1A | done | 1 | a9a12b5 | resumed after pause |
| WP-01F | 1 | fix | done | 1 | 42c7c06 | migration backfill = not archived; backfill test |
| WP-02 | 2 | 2A | done | 1 | c405e23 | Task.ci_autofix not persisted yet (WP-03); AutofixDefault not re-exported from core lib.rs |
| WP-04 | 2 | 2A | done | 1 | af18699 | claim_autofix takes &str; forward() paused check now after prompt build |
| WP-03 | 2 | 2B | done | 1 | c897f5c | +prs.rs test literal (allowed); CAS by value in BEGIN IMMEDIATE |
| WP-05 | 2 | 2C | done | 1 | 3f4e2b7 | +run.rs test literal (compile-only); PATCH uses set_ci_autofix, BacklogChange untouched |
| WP-05F | 2 | fix | done | 1 | a8d0b61 | Phase 2 review fixes |
| WP-06 | 3 | 3A | done | 1 | d7fd9ad | prompt duplicated in autofix (forward::ci_prompt/fit private) → dedupe in WP-07; FR-006 vs R3: followed R3 |
| WP-08 | 3 | 3A | done | 1 | dabc503 | manual forward: CAS sha before send, restore on failure, 409 after 3 CAS tries |
| WP-07 | 3 | 3B | done | 1 | 39c4788 | no re-read from gh before send (uses just-saved snapshot, FR-006?); stopped notice hook for WP-12 |
| WP-09 | 3 | 3C | done | 1 | 3a4ee36 | POST /runs has no ci_autofix (test uses PATCH) |
| WP-09F | 3 | fix | done | 1 | 9ee01f0 | Phase 3 review fixes; manual forward holds per-task autofix slot (InFlight → 409) |
| WP-10 | 4 | 4A | verified | 1 | f65133f | turn start unknown: started=None proxy (first turn end after sent_at counts) |
| WP-11 | 4 | 4B | verified | 1 | dc3dc77 | no back-to-backlog action exists; toggle-off caller → WP-14; delete drops the cancel event with the task |
| WP-12 | 4 | 4B | verified | 1 | | +blocking fixture, api.ts InboxKind/BLOCKING, alerts.ts label (es only like neighbours) |
| WP-13 | 4 | 4C | implementing | 0 | | |
| WP-14 | 5 | 5A | pending | 0 | | |
| WP-15 | 5 | 5A | pending | 0 | | |
| WP-16 | 5 | 5A | pending | 0 | | |
| WP-17 | 6 | 6A | pending | 0 | | |
| WP-18 | 7 | 7A | pending | 0 | | |
| WP-19 | 7 | 7B | pending | 0 | | |

## Open findings

| Phase | Reviewer | Severity | Finding | File |
|-------|----------|----------|---------|------|
| 1 | architecture | LOW | `pub(crate) mod api;` vs autopilot's private `mod api;` + re-export | crates/delilah-daemon/src/autofix/mod.rs:5 |
| 1 | architecture | LOW | Placeholder fns with `#[allow(dead_code)]`; drop the attribute as each gets a caller (`cancel` not in plan's mod.rs description) | crates/delilah-daemon/src/autofix/mod.rs:8-15 |
| 1 | migration | LOW | Backfill also matches autopilot `paused`; confirm intended | crates/delilah-daemon/migrations/0022_ci_autofix.sql:9 |
| 1 | migration | LOW | PRs with NULL snapshot get no seeded sha: a pre-upgrade failure could fire on first check; PrAutofix.sha must accept null | crates/delilah-daemon/migrations/0022_ci_autofix.sql:16-19 |
| 2-4 | orchestrator | MEDIUM | Flaky PTY-timing tests under load: `conversation::tests::a_message_sent_while_a_card_is_pending_is_queued`, `inbox::tests::a_reply_of_several_lines_is_one_paste_and_one_enter`, setup::tests::retry_runs_it_again_in_the_worktree, run::tests::free_tasks_start_branchless_and_jira_ones_on_their_feature_branch (fails ~1 in 3–4 full runs); file untouched by this feature | crates/delilah-daemon/src/conversation.rs:2257 |
| 2 | contract | HIGH (tracked) | UI settings PUT drops `ci_autofix` → resets to autopilot; must be fixed by WP-16 (TS type + DEFAULT_PROJECT_SETTINGS + round trip) before release | app/src/api.ts:101-109 |
| 2 | contract | MEDIUM | Version skew: new CLI vs old daemon 422 on project set (deny_unknown_fields); ship together, note in release notes; event downgrade also fails | crates/delilah-cli/src/main.rs:924 |
| 2 | spec-compliance | MEDIUM (tracked) | PATCH `ci_autofix` flips the column without the toggle's side effects/409 → WP-14 must route PATCH through the same toggle fn or limit it to backlog | crates/delilah-daemon/src/api.rs:563-566 |
| 2 | spec-compliance | LOW | Core T003 test checks a serde parse error; field/code only checked in daemon test | crates/delilah-core/src/project.rs |
| 2 | spec-compliance | LOW | PrAutofix.sha is Option (deliberate, backfill null head) | crates/delilah-daemon/src/autofix/mod.rs |
| 3 | security | MEDIUM | Check names/URLs are attacker-controlled and now reach the agent without a human; only prompt framing defends. Document (no bypass modes on untrusted PRs), consider non-fork only, "don't fetch URLs" | crates/delilah-daemon/src/autofix/mod.rs prompt() |
| 3 | security | LOW | fit() truncates the end, cutting the trailing instructions; cap listed checks / reserve footer | crates/delilah-daemon/src/pr_api/forward.rs fit() |
| 3 | security | LOW | safe_url allows any https host; one_line keeps U+2028/2029 | crates/delilah-daemon/src/pr_api/forward.rs |
| 3 | security | LOW | Green↔red cycles over new commits reset attempts: cost bounded per commit, not overall | crates/delilah-daemon/src/autofix/mod.rs |
| 3 | architecture | MEDIUM | pr_loop hard-wires autofix::evaluate (detached spawn, no join handle in tests); consider an AppState hook/channel | crates/delilah-daemon/src/pr_loop/mod.rs:208-214 |
| 3 | architecture | LOW | forward helpers (send/fit/safe_url) reached from autofix; move to a neutral module | crates/delilah-daemon/src/pr_api/forward.rs |
| 3 | architecture | LOW | autofix_handles reads the store on every notice record, and the rule lives outside autofix | crates/delilah-daemon/src/pr_loop/notices.rs:120-132 |
| 3 | spec-compliance | LOW | 4th red head stops at limit without recording attempt 3's autofix-failed-again (FR-013) → consider in Phase 4 | crates/delilah-daemon/src/autofix/mod.rs |
| 3 | spec-compliance | LOW | Manual forward of an older ci-failed notice overwrites sha with the older one | crates/delilah-daemon/src/pr_api/forward.rs |
| 3 | contract | LOW | TS `autofix?: PrAutofixView \| null` must be optional+nullable (WP-16) | app/src/api.ts |
| 3 | spec-compliance | LOW | still_due re-checks the stored snapshot (no gh call, by design R1/R3): record in decisions.md (WP-18) | crates/delilah-daemon/src/autofix/mod.rs |
| 3 | spec-compliance | LOW | Idle not re-checked between claim and send | crates/delilah-daemon/src/autofix/mod.rs |
| 3 | architecture | LOW | evaluate that loses claim_autofix returns silently (dropped until next poll); consider dirty flag / re-run on guard drop | crates/delilah-daemon/src/autofix/mod.rs |
| 3 | architecture | LOW | Two-way dependency autofix ↔ pr_api::forward; move delivery primitives to a shared module | crates/delilah-daemon/src/pr_api/forward.rs |
| 3 | architecture | LOW | RunRequest.ci_autofix must be exposed in app/src/api.ts (WP-16) | app/src/api.ts |
