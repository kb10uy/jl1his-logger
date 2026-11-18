DROP INDEX qsos_logbook_qsl_sent;

CREATE INDEX qsos_logbook_qsl_sent_upper ON qsos (logbook_id, upper(trim(coalesce(qsl_sent, ''))), qso_time)
WHERE deleted_at IS NULL;
