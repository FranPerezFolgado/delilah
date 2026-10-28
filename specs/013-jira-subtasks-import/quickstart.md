# Quickstart: try the Jira import in a preview

Never on the installed app (7420). From this worktree:

```bash
scripts/preview.sh --fresh          # prints UI http://localhost:150N and API http://127.0.0.1:750N
```

1. In the UI, connect Jira with **sample data**, and register a fixture repo as a project.
2. Pick a sample ticket with sub-tasks (one done, one assigned to someone else, one open).
   "Create parent task" → choose the project → check the default ticks → confirm.
   Expect: a parent task and only the open, mine-or-unassigned subtasks, in the backlog,
   nested in the sidebar.
3. Run "Create parent task" again on the same ticket: the dialog says the parent task exists,
   the created ones are "linked", nothing new is preselected.
4. Open the parent task: no offer if nothing is new. Skip one remaining sub-task from the
   dialog, reopen: it is not offered; "Skipped (1)" shows it with "Undo".
5. API/CLI:

```bash
API=http://127.0.0.1:750N
curl -s $API/api/v1/jira/issues/<KEY>/import | jq '.parent, [.subtasks[] | {key, action, selected}]'
DELILAH_URL=$API target/debug/delilah jira import <KEY>                 # preview only
DELILAH_URL=$API target/debug/delilah jira import <KEY> --project <P> --defaults
```

6. Start the parent task, then a subtask: its `~/.delilah`-style `tasks/<KEY>/ticket.md` (under
   the preview home) has the sub-task and a `Parent:` line.

Checks: `cargo test -p delilah-core -p delilah-integrations -p delilah-daemon -p delilah-cli`;
`cd app && npx tsc --noEmit && npx vitest run`.
