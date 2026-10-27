-- no-transaction
PRAGMA foreign_keys = OFF;

BEGIN;

DELETE FROM api_token_scopes WHERE token_id IN (SELECT id FROM api_tokens WHERE kind = 'wl2');
DELETE FROM api_tokens WHERE kind = 'wl2';
ALTER TABLE api_tokens DROP COLUMN kind;

CREATE TABLE stations_new (
    id TEXT PRIMARY KEY NOT NULL,
    logbook_id TEXT NOT NULL REFERENCES logbooks (id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    callsign TEXT NOT NULL,
    fields TEXT NOT NULL DEFAULT '{}' CHECK (json_valid(fields) AND json_type(fields) = 'object'),
    archived_at TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    UNIQUE (logbook_id, id)
);

INSERT INTO stations_new (id, logbook_id, name, callsign, fields, archived_at, created_at, updated_at)
SELECT id, logbook_id, name, callsign, fields, archived_at, created_at, updated_at FROM stations;

DROP TABLE stations;
ALTER TABLE stations_new RENAME TO stations;

CREATE INDEX stations_logbook_id ON stations (logbook_id);

COMMIT;

PRAGMA foreign_keys = ON;
