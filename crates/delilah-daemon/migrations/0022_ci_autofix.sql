-- CI autofix of the task: Delilah sends its PRs' CI failures to the agent on its own.
ALTER TABLE tasks ADD COLUMN ci_autofix INTEGER NOT NULL DEFAULT 0;
-- Autofix state of the PR, as JSON; NULL until autofix first acts on it.
ALTER TABLE pull_requests ADD COLUMN autofix TEXT;
-- Watched tasks already on autopilot get it, as the project default "autopilot" would.
-- `status` is JSON and there is no archived column: archived = terminal (done or failed)
-- and no workspace, as `open_prs` decides. A done task that keeps its workspace is still
-- watched; setup-failed is not terminal, so it is included too…
UPDATE tasks SET ci_autofix = 1
 WHERE autopilot IS NOT NULL
   AND NOT (json_extract(status, '$.type') IN ('done', 'failed')
            AND NOT EXISTS (SELECT 1 FROM workspaces w WHERE w.task_key = tasks.key));
-- …without firing at a failure from before the upgrade: their PRs' current head counts
-- as already forwarded.
UPDATE pull_requests
   SET autofix = json_object('state', 'watching', 'attempts', 0,
                             'sha', json_extract(snapshot, '$.head'))
 WHERE snapshot IS NOT NULL
   AND task_key IN (SELECT key FROM tasks WHERE ci_autofix = 1);
