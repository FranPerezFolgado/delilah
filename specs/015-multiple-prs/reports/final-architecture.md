# Final architecture review: 015-multiple-prs (phases 3-5)

Verdict: no CRITICAL. 2 HIGH, 3 MEDIUM, 2 LOW. Structure matches the plan (capture.rs
keeps phases.rs a dispatcher, `pr_repos` is the single source of "where PRs are looked
for", counted() is shared). The risks are in the archive gate.

[HIGH] crates/delilah-daemon/src/pr_loop/mod.rs:242 (pre_archive_detect, 281-285)
Problem: the pre-archive detection is fail-open. `detect` returns Err when gh fails
(repo_view / pr_list), and the loop ignores it (`if let Ok(l)`), so an outage or auth
error on one repo still lets `all_merged` archive and delete branches, which is the
exact early-archive bug FR-011 is meant to close. The forced path (`force == true`
skips pre_archive_detect and relies on the detection earlier in run_check) has the same
hole.
Fix: make pre_archive_detect return whether every repo was searched OK; if any repo
errored, skip on_merged this round (the 30 s poll retries). Same for the force path.

[HIGH] crates/delilah-daemon/src/pr_loop/mod.rs:236-255 (archive vs Working, TOCTOU)
Problem: primary_working is read once before the awaited forced detection (several gh
round-trips, seconds). The agent can start a turn and run `gh pr create` meanwhile; the
capture task is spawned outside the per-task claim, and `rows` is re-read only when
detection itself linked something. The archive then runs with a stale row set and
a now-Working agent (release deletes branches / archives the workspace under it).
Fix: after pre_archive_detect, always re-read rows and re-check `primary_working`
immediately before `merge::on_merged`. Optionally have capture take the same claim (or
set a per-task "capturing" flag that run_check honours).

[MEDIUM] crates/delilah-daemon/src/pr_loop/capture.rs:93 + store_prs.rs:324 (capture vs detection)
Problem: both paths can pass their `existing.any(url)` check and call link_pr_on_branch
for the same PR. The unique key keeps one row, but ON CONFLICT overwrites `repo` and
`linked_by`, so the last writer decides the label nondeterministically. Worse, capture's
`repo_of` falls back to the `host/owner/name` label when `repo_view` merely fails (the
transient error is indistinguishable from "not this repo"); detection then never fixes
it (it skips known URLs) and archive_heads ignores that PR, so its local branch is
neither verified nor deleted.
Fix: in the upsert keep the existing repo when the incoming one is the foreign form
(or never overwrite repo/linked_by on conflict); in repo_of return an error, not the
foreign label, when repo_view failed for every candidate.

[MEDIUM] crates/delilah-daemon/src/pr_loop/mod.rs:236-247 (retry cost when archive is blocked)
Problem: when release returns Blocked the rows stay all-merged, so every 30 s poll
repeats a forced detection over all pr_repos (gh calls, bypassing the back-off) for as
long as the task stays blocked.
Fix: remember the last forced pre-archive pass per task (timestamp in AppState) and
skip it within a few minutes, or do it only when archive-blocked is not already open.

[MEDIUM] crates/delilah-daemon/src/pr_loop/mod.rs:263 (primary_working)
Problem: Working is also the state of held stops with background work and of shell/setup
sessions (sessions.rs:337); a task whose primary session is stuck Working never archives
and no notice says why.
Fix: restrict to agent sessions and consider a staleness bound or a visible "waiting
for the agent" reason.

[LOW] crates/delilah-daemon/src/phases.rs:163
Problem: the turn-end nudge does a prs_of query on every TurnEnded for every task,
including non-primary and held replays; cheap but one more store hit per turn.
Fix: nudge from the pr_loop (which already no-ops without PRs) or restrict to `primary`.

[LOW] crates/delilah-daemon/src/pr_loop/mod.rs:469 (repo_of)
Problem: three unrelated resolution strategies (workspace, plain project, parent
project with submodules) and a `pr_repos`-vs-`repos()` split on has_branch live in one
function, each calling `gh repo view` serially per candidate.
Fix: split into a resolver returning candidate (label, dir) pairs, and keep the
gh matching loop single; also call repo_view once per dir for capture of several URLs.
