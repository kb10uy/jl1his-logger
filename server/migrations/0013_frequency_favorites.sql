CREATE TABLE frequency_favorites (
    id TEXT PRIMARY KEY NOT NULL,
    user_id TEXT NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    freq TEXT NOT NULL,
    freq_hz INTEGER NOT NULL CHECK (freq_hz > 0),
    mode TEXT NOT NULL,
    submode TEXT NOT NULL DEFAULT '',
    created_at TEXT NOT NULL,
    UNIQUE (user_id, freq_hz, mode, submode)
);
