CREATE TABLE station_favorites (
    logbook_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    station_id TEXT NOT NULL,
    created_at TEXT NOT NULL,
    PRIMARY KEY (user_id, station_id),
    FOREIGN KEY (logbook_id, user_id) REFERENCES memberships (logbook_id, user_id) ON DELETE CASCADE,
    FOREIGN KEY (logbook_id, station_id) REFERENCES stations (logbook_id, id) ON DELETE CASCADE
);

CREATE INDEX station_favorites_logbook_user ON station_favorites (logbook_id, user_id);

CREATE TABLE instrument_favorites (
    logbook_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    instrument_id TEXT NOT NULL,
    created_at TEXT NOT NULL,
    PRIMARY KEY (user_id, instrument_id),
    FOREIGN KEY (logbook_id, user_id) REFERENCES memberships (logbook_id, user_id) ON DELETE CASCADE,
    FOREIGN KEY (logbook_id, instrument_id) REFERENCES instruments (logbook_id, id) ON DELETE CASCADE
);

CREATE INDEX instrument_favorites_logbook_user ON instrument_favorites (logbook_id, user_id);
