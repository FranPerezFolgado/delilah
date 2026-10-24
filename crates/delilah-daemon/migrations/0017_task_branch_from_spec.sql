-- Task name and branch from the first analysis.
-- Where the title came from: initial (placeholder), spec, manual. Old tasks are manual.
ALTER TABLE tasks ADD COLUMN title_source TEXT NOT NULL DEFAULT 'manual';
-- The title an automatic rename replaced; NULL once the user changes the title.
ALTER TABLE tasks ADD COLUMN previous_title TEXT;
-- Why the branch couldn't be created; NULL when there is no problem.
ALTER TABLE tasks ADD COLUMN branch_error TEXT;
-- JSON array of the spec folder names present at start (`label:name`); NULL for a workspace started with a branch.
ALTER TABLE workspaces ADD COLUMN spec_dirs_at_start TEXT;
-- 1 when the workspace started without a branch (a free task with nothing typed and no template): only those get the branch steps. A task that started on a branch keeps it.
ALTER TABLE workspaces ADD COLUMN started_branchless INTEGER NOT NULL DEFAULT 0;
