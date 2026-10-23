-- Per-project settings (JSON of ProjectSettings), kept by Delilah and never written
-- into the project's repo. '{}' reads as the defaults.
ALTER TABLE projects ADD COLUMN settings TEXT NOT NULL DEFAULT '{}';
