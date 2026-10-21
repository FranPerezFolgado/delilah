-- One PR linked to a task, per repo of its workspace.
CREATE TABLE pull_requests (
    id                  INTEGER PRIMARY KEY,
    task_key            TEXT NOT NULL REFERENCES tasks (key) ON DELETE CASCADE,
    -- The repo inside the workspace: '.' or a submodule path.
    repo                TEXT NOT NULL,
    host                TEXT NOT NULL,
    -- owner/name
    owner_name          TEXT NOT NULL,
    number              INTEGER NOT NULL,
    url                 TEXT NOT NULL,
    -- detected, opened or pasted
    linked_by           TEXT NOT NULL,
    -- JSON Snapshot, NULL until the first check
    snapshot            TEXT,
    -- Head already notified as failed.
    ci_failed_sha       TEXT,
    review_seen         INTEGER NOT NULL DEFAULT 0,
    issue_comment_seen  INTEGER NOT NULL DEFAULT 0,
    review_comment_seen INTEGER NOT NULL DEFAULT 0,
    -- Last successful check (RFC 3339).
    checked_at          TEXT,
    -- Last check's problem, NULL when fine.
    error               TEXT,
    created_at          TEXT NOT NULL,
    UNIQUE (task_key, repo)
);

-- PRs the user unlinked: never detected again for that task.
CREATE TABLE pr_unlinked (
    task_key TEXT NOT NULL REFERENCES tasks (key) ON DELETE CASCADE,
    url      TEXT NOT NULL,
    PRIMARY KEY (task_key, url)
);

-- What the Bandeja shows for PRs.
CREATE TABLE pr_notices (
    id          INTEGER PRIMARY KEY,
    task_key    TEXT NOT NULL REFERENCES tasks (key) ON DELETE CASCADE,
    repo        TEXT NOT NULL,
    -- ci-failed, changes-requested, comments, archive-blocked, archive-partial
    kind        TEXT NOT NULL,
    -- Head sha, review id, highest comment id, or 'merge'.
    cause       TEXT NOT NULL,
    text        TEXT NOT NULL,
    -- JSON: failing checks or the comments.
    detail      TEXT,
    created_at  TEXT NOT NULL,
    -- NULL while it shows.
    resolved_at TEXT,
    UNIQUE (task_key, repo, kind, cause)
);
