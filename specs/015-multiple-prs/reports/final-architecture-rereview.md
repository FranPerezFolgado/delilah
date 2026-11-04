# Architecture re-review of WP-20F (two HIGH fixes)

Verdict: HIGH 1 (fail-open on gh error) is fixed for the unforced path only. Forced path still fails open: 1 HIGH remains. HIGH 2 (stale state) is fixed.

## Fixed
- pre_archive_detect (pr_loop/mod.rs:283) now returns `searched`; any `detect` Err makes it false and run_check (l.253) skips the archive and retries next poll. Correct for the unforced path.
- Stale state: rows are re-read after detection (l.252) and `primary_working` is re-checked (l.253). No await sits between that check and `on_merged`, so no window is left in that function. The claim guard is held throughout and nothing re-claims, so there is no deadlock. The cost is only that gh calls run longer under the claim.
- detect now links with `link_pr_on_branch(.., Some(branch))`, so a detected PR replaces the closed one of the same branch.

## Remaining
[HIGH] crates/delilah-daemon/src/pr_loop/mod.rs:209-216 and 245
Problem: on `force=true` (check_task_waiting and other forced callers) pre_archive_detect is skipped. The only detection is the loop at l.213, which discards errors (`if let Ok`). A gh failure there (repo_view or pr_list) leaves a repo unsearched, and l.258 still archives, because `safe` stays true. The fail-closed guarantee therefore does not hold for forced checks. The same case occurs when a task has no branch row or a store error: pre_archive_detect returns searched=true by default (l.285-288), though that is a lesser concern.
Fix: have the l.209 loop record `searched &= detect(..).is_ok()` (only for repos it actually tried), and compute `safe = searched && !primary_working` for both paths. The forced path can then skip the second detection but not the fail-closed check. Add a test: forced check, pr_list failing (DirGh fail_list), all rows merged, expect the task not archived.

## Tests
The three new tests (failed search keeps the task, a turn started during the search blocks the archive, detected PR replaces the closed one) exercise the unforced path and the races well. No test covers the forced path with a gh error. The implementer states RED was not observed, so I cannot confirm that the tests fail without the fix.

## Third re-review: pre_archive_detect fail-open (check_task, WP-20F fix round 2)

Verdict: FIXED. No CRITICAL/HIGH.

- Only call site of `merge::on_merged` is `pr_loop/mod.rs:262`, gated by `all_merged && !primary_working` and `safe`.
- Forced: every repo is detected with `force=true` (no `continue`, no back-off), so `searched` covers all repos; any Err (repo_view, pr_list, link failure) gives `searched=false`, `safe=false`, task kept. `safe` also re-checks `primary_working` after the slow search.
- Unforced: `safe` is overwritten by `pre_archive_detect` (forced detection of all `pr_repos`; Err gives `searched=false`). The first loop's Err from a backed-off repo (`!due`, only returned when `!force`) is discarded, so it neither blocks nor allows: the forced pass decides. Branchless/no workspace returns searched=true (no PR to find): correct.
- Race: `primary_working` is re-read after the detect and `all_merged` is re-evaluated on refreshed rows; the claim guard serialises checks. Residual (LOW, pre-existing): a turn starting between the final check and `on_merged` is not excluded.
