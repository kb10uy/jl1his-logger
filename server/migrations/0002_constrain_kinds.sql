CREATE TABLE export_batches_new (
    id TEXT PRIMARY KEY NOT NULL,
    logbook_id TEXT NOT NULL REFERENCES logbooks (id) ON DELETE CASCADE,
    kind TEXT NOT NULL CHECK (kind IN ('adif', 'pota', 'eqsl', 'lotw', 'qsl_card')),
    created_by TEXT NOT NULL REFERENCES users (id),
    created_at TEXT NOT NULL,
    applied_at TEXT,
    UNIQUE (logbook_id, id)
);

INSERT INTO export_batches_new (id, logbook_id, kind, created_by, created_at, applied_at)
SELECT id, logbook_id, kind, created_by, created_at, applied_at FROM export_batches;

CREATE TABLE export_batch_qsos_new (
    logbook_id TEXT NOT NULL,
    batch_id TEXT NOT NULL,
    qso_id TEXT NOT NULL,
    revision INTEGER NOT NULL,
    PRIMARY KEY (batch_id, qso_id),
    FOREIGN KEY (logbook_id, batch_id) REFERENCES export_batches_new (logbook_id, id) ON DELETE CASCADE,
    FOREIGN KEY (logbook_id, qso_id) REFERENCES qsos (logbook_id, id) ON DELETE CASCADE
);

INSERT INTO export_batch_qsos_new (logbook_id, batch_id, qso_id, revision)
SELECT logbook_id, batch_id, qso_id, revision FROM export_batch_qsos;

DROP TABLE export_batch_qsos;
DROP TABLE export_batches;
ALTER TABLE export_batches_new RENAME TO export_batches;
ALTER TABLE export_batch_qsos_new RENAME TO export_batch_qsos;

CREATE INDEX export_batches_logbook_id ON export_batches (logbook_id, created_at);

CREATE TABLE reference_versions_new (
    kind TEXT PRIMARY KEY NOT NULL CHECK (kind IN ('cty', 'subdivisions', 'parks')),
    version TEXT NOT NULL,
    checksum BLOB,
    updated_at TEXT NOT NULL
);

INSERT INTO reference_versions_new (kind, version, checksum, updated_at)
SELECT kind, version, checksum, updated_at FROM reference_versions;

DROP TABLE reference_versions;
ALTER TABLE reference_versions_new RENAME TO reference_versions;
