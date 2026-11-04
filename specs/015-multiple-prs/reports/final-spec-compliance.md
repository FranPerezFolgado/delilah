## Spec compliance - 015-multiple-prs (final, phases 1-6)

Method: read spec.md against final.diff and the touched files; no tests were run.

Covered (17 of 19 FR fully; see findings for the rest)
- FR-001/005/019: migration 0027_pr_per_url.sql rebuilds pull_requests with UNIQUE(task_key,url), copies ids and data, and re-points PR-kind notices by url.
- FR-002: Workspace::pr_repos (crates/delilah-git/src/workspace.rs:52) adds "." when the parent is on the task branch; used by detect (pr_loop/mod.rs:204, 287).
- FR-003/004: pr_loop/capture.rs (is_pr_create, urls_in; only `gh pr create` Bash PostToolUse), wired in phases.rs:172. Tests: ignores other commands/tools, stderr/stdout, dedupe.
- FR-006: pr_unlinked checked in detect (mod.rs:353) and capture (capture.rs:~82); cleared on manual link (store_prs.rs:341).
- FR-007/008: repo_of (mod.rs:480) labels workspace repo, "." or host/owner/name.
- FR-010/011/012: all_merged uses counted rows and requires at least one (merge.rs:26); pre_archive_detect runs forced detection (mod.rs:241-255); primary_working defers (mod.rs:241, 266).
- FR-014/015/016/018: PrView groups by repo, shows replaced/agent chips; summary() counts only counted PRs (prs.rs); open.rs offers "." for a parent on its branch; Spanish source strings with EN map (en/OpenPrDialog.ts, en/PrView.ts).
- FR-017: inbox.rs, autofix/mod.rs and autofix/api.rs moved to per-PR (pr_url, pr id) - spot-checked only.
- US2 scenario 5 (merged + closed-unmerged blocks), US3 scenario 4 (unlink clears block; store_prs.rs:395-420): covered.

Missing / Deviates

1. HIGH - FR-009 not met on the detection route.
   Replacement is only applied in link_pr_on_branch (store_prs.rs:306, 367), which is called only from link_known (pr_loop/mod.rs:466: pasted, opened, agent). detect() calls plain link_pr (pr_loop/mod.rs:368), which passes no head branch, and record() never replaces. If a PR is closed unmerged and a new PR from the same repo and branch is found by branch detection (push + web UI, or the hook missed it), the closed one stays counted: it blocks the archive and the badge forever (until manual unlink), and its notices stay open. Spec edge case and FR-009 say it must be marked replaced. Untested for this route (only the link_known route and the store function are tested).
   Fix: after the snapshot of the detected PR is known (or by reading it in detect), call the same replace logic; or run replace_closed in record() once snapshot.head_branch is known.

2. HIGH (data-loss risk, not run) - FR-013 / archive verification skips "." .
   merge.rs release() builds heads from pr_repos (includes "."), but verify() and delilah_git::check_merged iterate ws.repos() (merge.rs verify; workspace.rs:875), which has no ".". So a merged parent-repo PR head is never fetched or checked as an ancestor of the local branch, and its commits-after-the-PR-head are not detected. archive.rs:126-143 then adds the "." tip and delete_branch does update-ref -d at that tip (workspace.rs:951), so unpushed work on the parent branch beyond the PR head could be deleted without the safety check the submodules get. check_merged and verify were not changed in the diff. Needs a test: parent PR merged, extra local commit on the parent branch -> archive must block.

3. LOW - archive.rs:126 inserts "." into the deleted branches whenever any repo was verified, even if the parent had no PR. FR-013 says a parent without a PR keeps its branch. Pre-existing code (not in this diff), but now contradicts the spec text; confirm it is intended (decision FR-038 of spec 003 may say otherwise).

Extra: none found beyond task scope (unlink tombstone table reused; PR ordering sort_for_display supports FR-014).

Untested: FR-009 via detection (item 1); "." in archive verification (item 2); SC-001 10/10 and SC-004 upgrade check are not automatable here (migration copy is by construction; no migration test seen for notice re-pointing - not verified).

Open items outside the diff: T047 partial, T048 version bump pending (expected, not a finding).

VERDICT: GAPS (2 HIGH, 1 LOW)
