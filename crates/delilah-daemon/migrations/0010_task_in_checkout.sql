-- A backlog task's choice of where it will work once started: 1 in the project's
-- checkout, 0 in a worktree, NULL not chosen yet.
ALTER TABLE tasks ADD COLUMN in_checkout INTEGER;
