CREATE INDEX qsos_logbook_call_base_newest ON qsos (logbook_id, call_base, qso_time DESC) WHERE deleted_at IS NULL;
