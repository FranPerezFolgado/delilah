-- What the user keeps with a task (free text, e.g. a backlog idea); passed to the
-- agent when the task starts.
ALTER TABLE tasks ADD COLUMN notes TEXT;
