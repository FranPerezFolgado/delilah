-- Jira sub-tasks the user removed from a parent task (spec 013). The pair (parent task key,
-- Jira key) stops the import from creating that subtask again. No foreign key: the record
-- must outlive the subtask row it refers to.
CREATE TABLE jira_subtask_skips (
    parent_task TEXT NOT NULL,
    jira_key TEXT NOT NULL,
    created_at TEXT NOT NULL,
    PRIMARY KEY (parent_task, jira_key)
);
