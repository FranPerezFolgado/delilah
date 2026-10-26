-- Avisos the user removed from the Bandeja ("Quitar", "Limpiar avisos"); restorable.
-- `id` is the computed item id (kind:task:source:since_ms), so a new occurrence of the
-- same cause has a new id and shows again.
CREATE TABLE inbox_dismissals (
    id           TEXT PRIMARY KEY,
    task_key     TEXT NOT NULL REFERENCES tasks (key) ON DELETE CASCADE,
    dismissed_at TEXT NOT NULL
);
CREATE INDEX inbox_dismissals_by_task ON inbox_dismissals (task_key);
