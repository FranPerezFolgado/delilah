## WP-20F — done
Tasks:    HIGH1 fail-closed pre-archive detect ✓, HIGH2 re-check after detection ✓, extra FR-009 (detect replaces closed PR) ✓
Files:    crates/delilah-daemon/src/pr_loop/mod.rs — pre_archive_detect returns (linked, saved, searched); archive only if searched, rows re-read and primary_working re-checked after detection; detect() now uses link_pr_on_branch(.., Some(branch)); 3 tests
          crates/delilah-daemon/src/pr_testkit.rs — DirGh: fail_list, on_list hook, with_head_ref
          crates/delilah-daemon/src/store_prs.rs — link_pr marked #[cfg(test)] (now unused outside tests; clippy dead_code)
Tests:    a_failed_search_before_the_archive_keeps_the_task, an_agent_that_starts_a_turn_during_the_search_is_not_archived, a_detected_pr_replaces_the_closed_one_of_the_same_branch. cargo test -p delilah-daemon pr_loop: 69 pass; clippy -D warnings clean; rustfmt applied to my files.
Notes:    RED was not observed separately for the tests (written alongside the implementation, run once green). Forced path (force=true) skips pre_archive_detect as before. "PR appears during detection" is covered by the unconditional row re-read after detection.

## Fix round 2
Forced path: the detect loop in check_task now tracks `searched` (false on any Err); `safe = searched && !primary_working` initially, and on non-forced checks pre_archive_detect's result overrides it (the normal loop's Errs there include "backed off", which must not block).
Test: a_failed_search_on_a_forced_check_keeps_the_task. RED observed (failed before fix), green after. pr_loop tests all pass, clippy and rustfmt clean.
