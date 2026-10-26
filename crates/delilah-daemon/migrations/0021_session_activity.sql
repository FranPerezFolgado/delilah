-- Last turn activity of a session, for the project view's conversation rows.
ALTER TABLE sessions ADD COLUMN last_activity_at TEXT;   -- RFC 3339, NULL until the next turn
ALTER TABLE sessions ADD COLUMN last_message TEXT;       -- clipped to 200 chars
-- A runtime of the session's own; NULL: the task's chat_mode decides.
ALTER TABLE sessions ADD COLUMN runtime TEXT;            -- 'pty' | 'stream' | NULL
