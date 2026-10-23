-- Autopilot of the task, as JSON ({"state":"running","since":"..."}); NULL if off.
ALTER TABLE tasks ADD COLUMN autopilot TEXT;
-- Command that starts the project's preview (autopilot runs it before the PRs); NULL if none.
ALTER TABLE projects ADD COLUMN preview_command TEXT;
