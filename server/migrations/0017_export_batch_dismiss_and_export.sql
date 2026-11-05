ALTER TABLE export_batches ADD COLUMN dismissed_at TEXT;
ALTER TABLE export_batches ADD COLUMN export_id TEXT;
UPDATE export_batches SET export_id = id;
CREATE INDEX export_batches_export_id ON export_batches (logbook_id, export_id);
