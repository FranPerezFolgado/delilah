-- Parent tasks and subtasks (spec 012). 'kind' is 'normal', 'parent_task' or 'subtask'
-- (every existing row is normal); 'parent_task' is the key of a subtask's parent task.
ALTER TABLE tasks ADD COLUMN kind TEXT NOT NULL DEFAULT 'normal';
ALTER TABLE tasks ADD COLUMN parent_task TEXT;
CREATE INDEX tasks_parent_task ON tasks(parent_task);
