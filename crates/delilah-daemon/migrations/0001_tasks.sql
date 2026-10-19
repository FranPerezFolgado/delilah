CREATE TABLE tasks (
    key        TEXT PRIMARY KEY,
    title      TEXT NOT NULL,
    workflow   TEXT NOT NULL,
    phase      TEXT,
    -- TaskStatus as JSON, e.g. {"type":"paused-by-limit","resume_at":"..."}
    status     TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE task_events (
    id       INTEGER PRIMARY KEY AUTOINCREMENT,
    task_key TEXT NOT NULL REFERENCES tasks (key) ON DELETE CASCADE,
    at       TEXT NOT NULL,
    -- Event as JSON, e.g. {"type":"limit-reached","resume_at":"..."}
    event    TEXT NOT NULL
);

CREATE INDEX task_events_by_task ON task_events (task_key, id);
