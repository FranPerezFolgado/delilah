-- What a session is waiting on you for, as its hooks last said (JSON: kind, text,
-- since), or NULL. Feeds the inbox; kept here so it survives a daemon restart.
ALTER TABLE sessions ADD COLUMN attention TEXT;
