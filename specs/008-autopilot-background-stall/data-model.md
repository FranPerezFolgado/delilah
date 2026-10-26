# Data model: autopilot and background work

No schema, migration or stored state changes. `tasks.autopilot` (`state`, `nudged`,
`reason`…) keeps its shape; what changes is when a turn end may write `nudged` or
`paused`.

## End-of-turn report (input, from Claude Code's `Stop` hook)

| Field | Type | Use |
|---|---|---|
| `hook_event_name` | `"Stop"` | classified as `Signal::TurnEnded` (unchanged) |
| `background_tasks` | array, optional | absent in older agents |
| `background_tasks[].id` | string | not used |
| `background_tasks[].type` | string (`subagent`, …) | not used: every kind counts |
| `background_tasks[].agent_type` | string | not used |
| `background_tasks[].description` | string | not used |
| `background_tasks[].status` | string | `"running"` → background work is running |

`background_running(hook) = background_tasks is an array && any(entry.status == "running")`

## Turn end of the main session, on autopilot (decision table)

| Step done (boundary) | Background running | Nudged this step | Result |
|---|---|---|---|
| yes | any | any | handoff / next step, as today |
| no | yes | any | nothing (no nudge, no pause, `nudged` untouched) |
| no | no | no | one nudge (`nudged = step`) |
| no | no | yes | pause with the step's reason |

Failed turns (`StopFailure`), limited turns, non-main sessions, tasks off autopilot:
unchanged.
