-- A task can have several PRs in one repo (spec 015): pull_requests is keyed by
-- (task_key, url) instead of (task_key, repo), and notices say which PR they are about.
-- SQLite cannot drop a table constraint, so both tables are rebuilt: new table first,
-- copy with explicit ids, drop the old one, rename. Nothing references either table, so
-- no foreign-key toggling is needed.

CREATE TABLE pull_requests_new (
    id                  INTEGER PRIMARY KEY,
    task_key            TEXT NOT NULL REFERENCES tasks (key) ON DELETE CASCADE,
    -- The repo inside the workspace: '.' or a submodule path, or host/owner/name outside it.
    repo                TEXT NOT NULL,
    host                TEXT NOT NULL,
    -- owner/name
    owner_name          TEXT NOT NULL,
    number              INTEGER NOT NULL,
    url                 TEXT NOT NULL,
    -- detected, opened, pasted or agent
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
    error_kind          TEXT,
    autofix             TEXT,
    -- Id of the PR that replaced this closed one; NULL = counted.
    replaced_by         INTEGER,
    UNIQUE (task_key, url)
);

INSERT INTO pull_requests_new
    (id, task_key, repo, host, owner_name, number, url, linked_by, snapshot, ci_failed_sha,
     review_seen, issue_comment_seen, review_comment_seen, checked_at, error, created_at,
     error_kind, autofix, replaced_by)
SELECT id, task_key, repo, host, owner_name, number, url, linked_by, snapshot, ci_failed_sha,
       review_seen, issue_comment_seen, review_comment_seen, checked_at, error, created_at,
       error_kind, autofix, NULL
  FROM pull_requests;

DROP TABLE pull_requests;
ALTER TABLE pull_requests_new RENAME TO pull_requests;

CREATE TABLE pr_notices_new (
    id          INTEGER PRIMARY KEY,
    task_key    TEXT NOT NULL REFERENCES tasks (key) ON DELETE CASCADE,
    repo        TEXT NOT NULL,
    -- ci-failed, changes-requested, comments, pr-opened, archive-blocked, archive-partial...
    kind        TEXT NOT NULL,
    -- Head sha, review id, highest comment id, or 'merge'.
    cause       TEXT NOT NULL,
    text        TEXT NOT NULL,
    -- JSON: failing checks or the comments.
    detail      TEXT,
    created_at  TEXT NOT NULL,
    -- NULL while it shows.
    resolved_at TEXT,
    -- The PR the notice is about; '' for task-level notices.
    pr_url      TEXT NOT NULL DEFAULT '',
    UNIQUE (task_key, repo, pr_url, kind, cause)
);

-- PR-kind notices take the url of their (task, repo) PR; the rest stay task-level. A
-- PR-kind notice whose PR is gone could never be resolved, so the migration resolves it.
INSERT INTO pr_notices_new
    (id, task_key, repo, kind, cause, text, detail, created_at, resolved_at, pr_url)
SELECT n.id, n.task_key, n.repo, n.kind, n.cause, n.text, n.detail, n.created_at,
       CASE WHEN n.kind IN ('ci-failed', 'changes-requested', 'comments', 'pr-opened', 'autofix-stopped')
                 AND n.resolved_at IS NULL
                 AND NOT EXISTS (SELECT 1 FROM pull_requests p
                                  WHERE p.task_key = n.task_key AND p.repo = n.repo)
            THEN strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
            ELSE n.resolved_at END,
       CASE WHEN n.kind IN ('ci-failed', 'changes-requested', 'comments', 'pr-opened', 'autofix-stopped')
            THEN COALESCE((SELECT p.url FROM pull_requests p
                            WHERE p.task_key = n.task_key AND p.repo = n.repo), '')
            ELSE '' END
  FROM pr_notices n;

DROP TABLE pr_notices;
ALTER TABLE pr_notices_new RENAME TO pr_notices;
