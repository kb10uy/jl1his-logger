CREATE TABLE sessions_new (
    id TEXT PRIMARY KEY NOT NULL,
    token_hash BLOB NOT NULL UNIQUE,
    user_id TEXT NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    user_agent TEXT,
    created_at TEXT NOT NULL,
    last_used_at TEXT NOT NULL,
    expires_at TEXT NOT NULL
);

INSERT INTO sessions_new (id, token_hash, user_id, user_agent, created_at, last_used_at, expires_at)
SELECT lower(hex(randomblob(16))), token_hash, user_id, NULL, created_at, created_at, expires_at FROM sessions;

DROP TABLE sessions;
ALTER TABLE sessions_new RENAME TO sessions;

CREATE INDEX sessions_user_id ON sessions (user_id);
