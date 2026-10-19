CREATE TABLE workspaces (
    task_key   TEXT PRIMARY KEY REFERENCES tasks (key) ON DELETE CASCADE,
    parent     TEXT NOT NULL,
    path       TEXT NOT NULL,
    branch     TEXT NOT NULL,
    -- JSON array of submodule paths
    submodules TEXT NOT NULL
);

CREATE TABLE agent_sessions (
    task_key   TEXT PRIMARY KEY REFERENCES tasks (key) ON DELETE CASCADE,
    agent      TEXT NOT NULL,
    -- Chosen by Delilah and passed with --session-id, so it can always --resume.
    session_id TEXT NOT NULL,
    started_at TEXT NOT NULL
);
