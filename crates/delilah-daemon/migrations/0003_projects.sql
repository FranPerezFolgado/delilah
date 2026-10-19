CREATE TABLE projects (
    name       TEXT PRIMARY KEY,
    path       TEXT NOT NULL UNIQUE,
    -- "parent" (submodules) or "repo"
    kind       TEXT NOT NULL,
    workflow   TEXT NOT NULL,
    created_at TEXT NOT NULL
);

ALTER TABLE tasks ADD COLUMN project TEXT REFERENCES projects (name);

-- Workspaces created before projects existed were all parent workspaces.
ALTER TABLE workspaces ADD COLUMN kind TEXT NOT NULL DEFAULT 'parent';
