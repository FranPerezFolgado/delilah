-- The kind of access problem behind `error` (missing, signed-out, no-access, network...).
ALTER TABLE pull_requests ADD COLUMN error_kind TEXT;
