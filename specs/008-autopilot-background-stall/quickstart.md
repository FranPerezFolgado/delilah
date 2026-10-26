# Quickstart: checking the fix

## Tests

```bash
cargo test -p delilah-agents background_running     # the payload rule
cargo test -p delilah-daemon autopilot               # stall decision through the hook endpoint
cargo test -p delilah-e2e --test autopilot           # fake Claude + isolated daemon, plan → implement
cargo test -p delilah-e2e                            # fake-claude steps and scenario lookup
```

The e2e test `autopilot_waits_on_background_work` must fail on `main` (it sees the
nudge and then the pause) and pass on this branch.

## By hand, in a preview

1. `scripts/preview.sh --fresh`, register a fixture repo, and set the preview's fake
   Claude, or run a real one.
2. Launch a `feature` task on autopilot with a real Claude: during `/feature` the plan
   reviewer runs in the background; the header keeps "Autopilot running" and the task's
   history has no "autopilot nudged"/"paused" while it waits.
3. In `/orchestrate`, implementers run in the background: same check.
4. A real stall (interrupt the agent with Esc so it ends a turn with nothing running):
   one nudge, then "Autopilot paused" with "N tasks unchecked".
