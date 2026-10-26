# Progress — 008-autopilot-background-stall

Branch: 008-autopilot-background-stall · Base: 686315d · Started: 2026-10-26
Mode: format=markdown commits=yes autonomy=auto

| WP | Phase | Group | Status | Attempts | Commit | Notes |
|----|-------|-------|--------|----------|--------|-------|
| WP-01 | 1 | 1A | done | 1 | 24c016c | |
| WP-02 | 2 | 2A | done | 1 | 8fe3ebd | |
| WP-03 | 2 | 2A | done | 1 | 03456b2 | |
| WP-04 | 2 | 2A | done | 1 | 929ef46 | |
| WP-05 | 3 | 3A | done | 1 | 8c8a7e3 | |
| WP-06 | 4 | 4A | verified | 2 | 7c14b5f | flake fixed: event polling + log signal |
| WP-07 | 4 | 4A | verified | 1 | be0ce95 | |
| WP-08 | 5 | 5A | verified | 1 | | |
| WP-09 | 6 | 6A | verified | 1 | db2cc68 | |
| WP-10 | 6 | 6B | pending | 0 | | |

## Open findings

| Phase | Reviewer | Severity | Finding | File |
|-------|----------|----------|---------|------|
| 2 | security | LOW | write-path check is lexical; a symlink already inside cwd could redirect a write (test-only, not realistic) | crates/delilah-e2e/src/fake/run.rs |
| 2 | spec-compliance | LOW | unknown keys inside a `stop` object (e.g. `backgroud`) are ignored silently, so a typo sends no background_tasks | crates/delilah-e2e/src/fake/scenario.rs:117 |
| 2 | spec-compliance | LOW | `"background": null` accepted as omitted; `command()` trims leading whitespace; `./` allowed in write paths (lenient extras) | crates/delilah-e2e/src/fake/scenario.rs:120,240; run.rs:199 |
| 2 | spec-compliance | LOW | `write_with_a_bad_path_exits_2` doesn't assert the error message is printed | crates/delilah-e2e/src/fake/run.rs:812 |

## WP-05 test-first evidence

Run on the unchanged daemon (before T011/T012):

```
cargo test -p delilah-daemon background_tests
FAILED a_plan_turn_end_with_background_work_is_not_a_stall
FAILED repeated_background_turn_ends_never_nudge_or_pause
FAILED an_implement_turn_end_with_background_work_is_not_a_stall
  hooks.rs:767: assertion `left == right` failed: Autopilot { state: Paused, reason: Some("2 tareas sin marcar"), .. }
  left: Paused   right: Running
ok     the_plan_handoff_still_starts_with_background_work_running (FR-004, passes today)

cargo test -p delilah-e2e --test autopilot
FAILED autopilot_waits_on_background_work: timed out after 30s waiting for the review step starts
  last value: autopilot {"nudged":"plan","reason":"tasks.md not written","state":"paused"},
  events ["start","workspace-ready","agent-asked","autopilot-nudged","autopilot-paused",...]
```
| 3 | architecture | MEDIUM | fixed 400ms sleep after spawned turn end; negative asserts can pass vacuously → folded into WP-06 | crates/delilah-daemon/src/hooks.rs:~765 |
| 3 | architecture | LOW | `on_turn_ended_with` only called by tests but not cfg(test); wrapper names differ by one letter from on_turn_end(_with) | crates/delilah-daemon/src/handoff.rs:241-259 |
| 3 | architecture | LOW | background_tests duplicates autopilot::testkit setup (~70 lines, testkit private) and leaks a temp dir per test → TempDir folded into WP-06 | crates/delilah-daemon/src/hooks.rs:670-740 |
| 3 | spec-compliance | LOW | FR-004 test passes on handoff record OR implement session (launch fails with /nonexistent/claude); writes only tasks.md | crates/delilah-daemon/src/hooks.rs:~808 |
| 3 | spec-compliance | LOW | `Api::set_preference` added without a task (test support to skip save-notes) | crates/delilah-e2e/src/api.rs:121 |
| 3 | test-reviewer | HIGH→planned | real-stall coverage (done/absent fixtures, running→empty, e2e nudge→pause) is T013/T014 in phase 4 | — |

## SC-003 proof (T015)

Method: the test and the fix landed together in 8c8a7e3, so a scratch detached worktree at HEAD (7c14b5f) had only `handoff.rs` and `phases.rs` restored to their 03456b2 (pre-fix) version, with its own CARGO_TARGET_DIR. Ran `cargo test -p delilah-e2e --test autopilot autopilot_waits_on_background_work`. The worktree was then removed.

Pre-fix result: FAILED. Timed out after 30s waiting for "the review step starts"; the task was `paused` (nudged "plan", reason "tasks.md not written"). Events: start, workspace-ready, agent-asked, autopilot-nudged, autopilot-paused, user-answered, agent-asked, phase-reached.

Head result (7c14b5f): `cargo test -p delilah-e2e --test autopilot` passes, 2 passed (autopilot_waits_on_background_work, autopilot_still_catches_a_real_stall).

T016: grep for `/Users/`, `LIBRE-`, `PROJ-` and hex ids of 9+ chars in the hook fixtures and `autopilot.rs` found no hits.
