-- Runtime of the task's Claude sessions: 0 = terminal (PTY), 1 = chat (stream-json).
ALTER TABLE tasks ADD COLUMN chat_mode INTEGER NOT NULL DEFAULT 0;
-- Permission rules granted with "allow always in this task": a JSON array of strings, NULL = none.
ALTER TABLE tasks ADD COLUMN allowed_tools TEXT;
