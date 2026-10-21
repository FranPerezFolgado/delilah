-- Idea conversations: a project's Claude session that turns an idea into tasks.
-- conversation = 1 implies project set and kind = 'claude' (enforced in code).
ALTER TABLE sessions ADD COLUMN conversation INTEGER NOT NULL DEFAULT 0;

CREATE TABLE conversation_entries (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id TEXT NOT NULL REFERENCES sessions (id) ON DELETE CASCADE,
    at         TEXT NOT NULL,           -- RFC 3339, when it was created
    kind       TEXT NOT NULL,           -- 'user' | 'claude' | 'card'
    body       TEXT NOT NULL            -- JSON, by kind
);
CREATE INDEX conversation_entries_by_session ON conversation_entries (session_id, id);
