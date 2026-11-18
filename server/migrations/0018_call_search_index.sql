DROP INDEX qsos_logbook_call;

CREATE INDEX qsos_logbook_call_upper ON qsos (logbook_id, upper(trim(coalesce(call, ''))), qso_time)
WHERE deleted_at IS NULL;
