-- Saving mode: each phase of the task runs in a fresh Claude session.
ALTER TABLE tasks ADD COLUMN saving_mode INTEGER NOT NULL DEFAULT 0;
-- The handoff under way, as JSON ({"to":"implement","stage":"offered"}); NULL if none.
ALTER TABLE tasks ADD COLUMN handoff TEXT;
-- The step of the saving-mode chain this session runs: plan, implement, review.
ALTER TABLE sessions ADD COLUMN step TEXT;
