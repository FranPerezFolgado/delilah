# Contract: what the daemon reads from a `Stop` hook

Input contract with Claude Code (not a Delilah API; nothing changes in `api.rs` or
`app/src/api.ts`).

```json
{
  "hook_event_name": "Stop",
  "session_id": "…",
  "last_assistant_message": "…",
  "background_tasks": [
    {"id": "a1", "type": "subagent", "agent_type": "implementer",
     "description": "WP-01", "status": "running"}
  ]
}
```

- `delilah_agents::claude::background_running(&hook) -> bool`: true iff
  `background_tasks` is an array with at least one entry whose `status` is the string
  `"running"`. Missing field, not an array, empty, or no running entry: false.
- Only used for `Signal::TurnEnded` of the task's main session, to decide whether the
  turn end can be an autopilot stall.
- Fixtures in the repo hold this shape with neutral values only.
