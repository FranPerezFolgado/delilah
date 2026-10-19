-- A session belongs to a task or, to ask about the code without one, to a project:
-- it then runs in the project's own checkout. `read_only` sessions start Claude in
-- plan mode (it reads and explains, it does not edit).
CREATE TABLE sessions_new (
    id         TEXT PRIMARY KEY,
    task_key   TEXT REFERENCES tasks (key) ON DELETE CASCADE,
    project    TEXT REFERENCES projects (name) ON DELETE CASCADE,
    kind       TEXT NOT NULL,
    label      TEXT NOT NULL,
    is_primary INTEGER NOT NULL DEFAULT 0,
    closed     INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL,
    attention  TEXT,
    read_only  INTEGER NOT NULL DEFAULT 0,
    CHECK ((task_key IS NULL) != (project IS NULL))
);

INSERT INTO sessions_new (id, task_key, kind, label, is_primary, closed, created_at, attention)
    SELECT id, task_key, kind, label, is_primary, closed, created_at, attention FROM sessions;

DROP TABLE sessions;
ALTER TABLE sessions_new RENAME TO sessions;
CREATE INDEX sessions_by_task ON sessions (task_key, created_at);
CREATE INDEX sessions_by_project ON sessions (project, created_at);
