CREATE TABLE reference_versions_new (
    kind TEXT PRIMARY KEY NOT NULL CHECK (kind IN ('cty', 'parks')),
    version TEXT NOT NULL,
    checksum BLOB,
    updated_at TEXT NOT NULL
);

INSERT INTO reference_versions_new (kind, version, checksum, updated_at)
SELECT kind, version, checksum, updated_at FROM reference_versions WHERE kind <> 'subdivisions';

DROP TABLE reference_versions;
ALTER TABLE reference_versions_new RENAME TO reference_versions;
