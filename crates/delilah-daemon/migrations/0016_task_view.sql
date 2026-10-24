-- How the app shows the task: 'chat' or 'terminal'. Display only.
ALTER TABLE tasks ADD COLUMN view TEXT NOT NULL DEFAULT 'chat';
