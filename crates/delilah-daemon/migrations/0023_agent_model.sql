-- The model the task's Claude sessions start on ('opus', 'sonnet', 'opusplan', 'haiku',
-- 'default' = no --model); NULL follows the phase's (the `agent-models` preference).
ALTER TABLE tasks ADD COLUMN model TEXT;
-- The model a session was started on, kept so a resume never switches it; NULL: none was
-- passed (also every session from before this column).
ALTER TABLE sessions ADD COLUMN model TEXT;
