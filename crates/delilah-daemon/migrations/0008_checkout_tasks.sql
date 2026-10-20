-- A task may work in the project's own checkout instead of a worktree. `previous`
-- holds, as JSON [[folder, branch-or-commit], …], where each folder was before, to
-- go back there when the task is archived; NULL for a worktree.
ALTER TABLE workspaces ADD COLUMN previous TEXT;
