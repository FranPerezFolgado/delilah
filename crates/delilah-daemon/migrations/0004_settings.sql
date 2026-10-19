-- Small, non-secret settings as JSON (e.g. the Jira site). Secrets live in the Keychain.
CREATE TABLE settings (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
