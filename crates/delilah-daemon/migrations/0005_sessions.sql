-- A task's workspace can host several sessions: its primary Claude, extra Claude
-- sessions (to review, or to look into something else) and plain shells.
CREATE TABLE sessions (
    -- For Claude sessions, also the id passed to claude --session-id / --resume.
    id         TEXT PRIMARY KEY,
    task_key   TEXT NOT NULL REFERENCES tasks (key) ON DELETE CASCADE,
    -- "claude" or "shell"
    kind       TEXT NOT NULL,
    label      TEXT NOT NULL,
    -- The primary session's turns drive the task's status.
    is_primary INTEGER NOT NULL DEFAULT 0,
    closed     INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL
);

CREATE INDEX sessions_by_task ON sessions (task_key, created_at);

INSERT INTO sessions (id, task_key, kind, label, is_primary, created_at)
    SELECT session_id, task_key, agent, 'Principal', 1, started_at FROM agent_sessions;

DROP TABLE agent_sessions;
