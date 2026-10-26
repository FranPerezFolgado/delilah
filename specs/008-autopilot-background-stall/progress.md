# Progress — 008-autopilot-background-stall

Branch: 008-autopilot-background-stall · Base: 686315d · Started: 2026-10-26
Mode: format=markdown commits=yes autonomy=auto

| WP | Phase | Group | Status | Attempts | Commit | Notes |
|----|-------|-------|--------|----------|--------|-------|
| WP-01 | 1 | 1A | done | 1 | 24c016c | |
| WP-02 | 2 | 2A | done | 1 | 8fe3ebd | |
| WP-03 | 2 | 2A | done | 1 | 03456b2 | |
| WP-04 | 2 | 2A | done | 1 | 929ef46 | |
| WP-05 | 3 | 3A | verified | 1 | | |
| WP-06 | 4 | 4A | pending | 0 | | |
| WP-07 | 4 | 4A | pending | 0 | | |
| WP-08 | 5 | 5A | pending | 0 | | |
| WP-09 | 6 | 6A | pending | 0 | | |
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
