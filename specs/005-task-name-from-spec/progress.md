# Progress — 005-task-name-from-spec

Branch: feature/LIBRE-11 · Base: 35a276d · Started: 2026-10-24
Mode: format=markdown commits=yes autonomy=auto

| WP | Phase | Group | Status | Attempts | Commit | Notes |
|----|-------|-------|--------|----------|--------|-------|
| WP-01 | 2 | 2A | done | 1 | b9cd4b4 | archived = Done/Failed with no workspace row; also touched delilah-core lib.rs (re-export) |
| WP-02 | 2 | 2A | done | 1 | 8286607 | |
| WP-03 | 2 | 2B | done | 1 | c472b6b | proposals.rs untouched (goes through build_task) |
| WP-04 | 3 | 3A | done | 1 | bd30a70 | store.rename_from_spec added (atomic, WHERE initial) |
| WP-05 | 3 | 3A | done | 1 | 2b5996b | run early with 2A (no deps) |
| WP-06 | 4 | 4A | verified | 1 | 5e05d63 | parent root not dirty-checked in branchless use_checkout (submodules only) |
| WP-10 | 4 | 4A | verified | 1 | 98a3637 | run early; alerts.ts allowed by user |
| WP-07 | 4 | 4B | verified | 1 | c808cb6 | ensure()+ensure_locked(); Activity waits while branch_error set |
| WP-09 | 4 | 4B | verified | 1 | 407a9a3 | no delilah-git change needed |
| WP-08 | 4 | 4C | verified | 1 | d792563 | guard on every free-task session outside start_run (returns at once once branched) |
| WP-4F | 4 | 4F | implementing | 0 | | phase 4 review fixes (round 1) |
| WP-11 | 5 | 5A | verified | 1 | PENDING | CHK016, CHK033-035 left open |
| WP-12 | 5 | 5B | pending | 0 | | quickstart walk via ui-verifier |

## Open findings

| Phase | Reviewer | Severity | Finding | File |
|-------|----------|----------|---------|------|
| 2 | orchestrator | LOW | Pre-existing flaky daemon tests (timing): conversation::tests::a_message_sent_while_a_card_is_pending_is_queued, pr_loop::tests::nudges_in_the_window_make_one_delayed_check, run::tests::a_started_task_from_a_conversation_reads_the_edited_brief | crates/delilah-daemon/src |
| 2 | migration | MEDIUM | save_workspace (INSERT OR REPLACE) omits spec_dirs_at_start, so any re-save resets it to NULL; use ON CONFLICT DO UPDATE or re-set after save | crates/delilah-daemon/src/store_runs.rs:19 |
| 2 | migration | MEDIUM | update_backlog_task writes back title_source/previous_title from a stale read; a concurrent set_title_source(Spec) is lost | crates/delilah-daemon/src/store.rs:~145 |
| 2 | migration | LOW | set_title archived check and UPDATE not atomic | crates/delilah-daemon/src/store.rs |
| 2 | migration | LOW | set_workspace_branch / set_spec_dirs_at_start return Ok when no row matched; branch can be overwritten | crates/delilah-daemon/src/store_runs.rs:33,50 |
| 2 | migration | LOW | unknown title_source makes task_from_row Corrupt for every list | crates/delilah-daemon/src/store.rs |
| 2 | architecture | MEDIUM | "free task" = LIBRE- prefix defined twice (is_free vs next_free_key); a Jira project keyed LIBRE would count as free | crates/delilah-daemon/src/branching.rs:30, run.rs:592 |
| 2 | architecture | MEDIUM | recover takes the per-task lock on every tick before a cheap pending check; lock held during the whole work | crates/delilah-daemon/src/scheduler.rs:40, branching.rs:107 |
| 2 | architecture | MEDIUM | on_activity/recover check a task snapshot read before the lock (stale title_source) — WP-04 told to re-read under lock | crates/delilah-daemon/src/branching.rs:84-100 |
| 2 | architecture | MEDIUM | on_hook spawns one on_activity per Working/TurnEnded with no coalescing | crates/delilah-daemon/src/phases.rs:158-163 |
| 2 | architecture | LOW | BranchLocks never shrinks; Initial-for-free duplicated in api.rs/run.rs; start_run overwrites spec_dirs baseline on a second start; recover_branches blocks startup; tick/recover_branches loop duplicated | branching.rs, api.rs:332, run.rs:232,305, main.rs:59, scheduler.rs:20 |
| 2 | spec-compliance | MEDIUM | T012 untested: no test that start_run/build_task create free tasks initial and Jira manual, or that spec_dirs_at_start is recorded (incl. submodules) | crates/delilah-daemon/src/run.rs, api.rs |
| 2 | spec-compliance | LOW | set_spec_dirs_at_start failure at run.rs:311 returns after save_workspace without Fail/StartAborted (task stuck Starting) | crates/delilah-daemon/src/run.rs:311 |
| 3 | contract | MEDIUM | retryBranch client/InboxView call POST /api/v1/tasks/{key}/branch before the route exists (lands in WP-08, same release) | app/src/api.ts:439 |
| 3 | contract | LOW | 409 for renaming an archived task not stated in contracts/api.md; BacklogChange comment in api.ts still says backlog-only | contracts/api.md, app/src/api.ts:93 |
| 3 | spec-compliance | LOW | title-only PATCH with an unchanged title still makes a spec title manual and clears previous_title (API/CLI) | crates/delilah-daemon/src/api.rs:369 |
| 3 | spec-compliance | LOW | title editable / Restaurar shown on archived tasks (409 on save) | app/src/components/TaskScreen.tsx:518-545 |
| 3 | spec-compliance | LOW | untested: PATCH archived → 409 at API level; retitle no-op on a Jira task with a new spec folder; title from a submodule's spec | api.rs, branching.rs |
| 3 | ui-verifier | MEDIUM | No browser tools: header edit, Escape, Antes/Restaurar not checked in a real browser (API behaviour + auto-rename verified in preview; vitest passes) | app/src/components/TaskScreen.tsx |
| 4 | orchestrator | MEDIUM | A submodule that already has a branch with the chosen name fails apply_branch → branch_error instead of being switched to it (needs a delilah-git checkout-existing helper) | crates/delilah-daemon/src/branching.rs |
| 4 | orchestrator | LOW | Sessions launched outside start_run (resume, extra sessions) get the guard hook for any free task even after the branch exists (R3 says they shouldn't); cost = one curl per edit/commit | crates/delilah-daemon/src/run.rs |
| 4 | security | MEDIUM | Bash guard is a substring match: misses `git -C . commit`, `git -c k=v commit`, double spaces, /usr/bin/git, scripts/aliases; guard is best-effort (archive still refuses detached commits) | crates/delilah-daemon/src/branching.rs:~372,~440 |
| 4 | security | LOW | specs/ .specify/ exemption is lexical: a symlink inside specs/ escapes the guard | crates/delilah-daemon/src/branching.rs:~408-428 |
| 4 | security | LOW | task/session/daemon_url interpolated unescaped into single-quoted curl (pre-existing pattern; values are safe today) | crates/delilah-agents/src/claude.rs:229-265 |
| 4 | security | LOW | /hooks/claude/guard unauthenticated like other hooks (any local process can trigger early branch creation; idempotent) | crates/delilah-daemon/src/hooks.rs:172-182 |
| 4 | architecture | MEDIUM | retry axum handler + guard_reply hook JSON live in branching.rs (domain module imports axum/api); move to api/hooks | crates/delilah-daemon/src/branching.rs:483-534 |
| 4 | architecture | MEDIUM | branching.rs ~600 non-test lines mixing locks, spec discovery, settle, guard policy, HTTP, retitle; split into branching/{settle,guard,spec} | crates/delilah-daemon/src/branching.rs |
| 4 | architecture | MEDIUM | retitle/spec_title/new_spec_folder do sync fs reads on the async runtime on every Working/TurnEnded | crates/delilah-daemon/src/branching.rs:63,77,539-567 |
| 4 | architecture | LOW | "caller holds the lock" only by doc comment (ensure_locked/retitle); stringly-typed Result<Option<String>,String>; launch_session vs launch_session_guarded decide guard with different rules; Initial-for-free duplicated | branching.rs:292-377, run.rs:432-447 |
| 4 | contract | LOW | retry 200 that fails again gives no feedback; 404/409 messages English-only (not lang().pick); delilah show lacks previous_title; retry doesn't 409 archived tasks | InboxView.tsx:103-114, branching.rs:508-513, delilah-cli/src/main.rs:280-292 |
| 4 | spec-compliance | LOW | "-2/-3" name chosen silently (spec: "and says so"); slug keeps a task key present in the title (FR-006) | crates/delilah-daemon/src/branching.rs:205 |
| 4 | spec-compliance | LOW | on_activity spawned (not synchronous with the hook) so branch not guaranteed before next tool; adopted sessions miss R8 sentence | phases.rs:159-163, run.rs:373 |
| 4 | spec-compliance | LOW | untested: checkout-mode free start via start_run; parent archive with descriptive branch | run.rs, archive.rs |
| 4 | ui-verifier | LOW | E2E (API) 5/6 pass; Bandeja branch item not exercised end to end. `preview.sh --fresh` left a stale prunable LIBRE-2 worktree in fixtures/signal from an earlier preview (first detached worktree add failed once) | scripts/preview.sh / fixtures |
