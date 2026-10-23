CREATE TABLE instruments (
    id TEXT PRIMARY KEY NOT NULL,
    logbook_id TEXT NOT NULL REFERENCES logbooks (id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    my_rig TEXT,
    my_antenna TEXT,
    tx_pwr TEXT,
    fields TEXT NOT NULL DEFAULT '{}' CHECK (json_valid(fields) AND json_type(fields) = 'object'),
    archived_at TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    UNIQUE (logbook_id, id)
);

CREATE INDEX instruments_logbook_id ON instruments (logbook_id);

CREATE TABLE qsos_new (
    id TEXT PRIMARY KEY NOT NULL,
    logbook_id TEXT NOT NULL REFERENCES logbooks (id) ON DELETE CASCADE,
    station_id TEXT NOT NULL,
    instrument_id TEXT,
    seq INTEGER NOT NULL,
    updated_seq INTEGER NOT NULL,
    revision INTEGER NOT NULL DEFAULT 1,
    created_by TEXT NOT NULL REFERENCES users (id),
    created_at TEXT NOT NULL,
    updated_by TEXT NOT NULL REFERENCES users (id),
    updated_at TEXT NOT NULL,
    deleted_at TEXT,

    call TEXT NOT NULL,
    qso_date TEXT NOT NULL,
    time_on TEXT NOT NULL,
    time_off TEXT,
    band TEXT NOT NULL,
    mode TEXT NOT NULL,
    submode TEXT,
    freq TEXT,
    rst_sent TEXT,
    rst_rcvd TEXT,
    station_callsign TEXT NOT NULL,
    operator TEXT,
    name TEXT,
    qth TEXT,
    gridsquare TEXT,
    state TEXT,
    cnty TEXT,
    dxcc TEXT,
    contest_id TEXT,
    stx_string TEXT,
    srx_string TEXT,
    comment TEXT,
    tx_pwr TEXT,

    qsl_sent TEXT,
    qsl_sent_via TEXT,
    qslsdate TEXT,
    qsl_rcvd TEXT,
    qsl_rcvd_via TEXT,
    qslrdate TEXT,
    eqsl_qsl_sent TEXT,
    eqsl_qslsdate TEXT,
    eqsl_qsl_rcvd TEXT,
    eqsl_qslrdate TEXT,
    lotw_qsl_sent TEXT,
    lotw_qslsdate TEXT,
    lotw_qsl_rcvd TEXT,
    lotw_qslrdate TEXT,

    my_gridsquare TEXT,
    my_city TEXT,
    my_state TEXT,
    my_cnty TEXT,
    my_pota_ref TEXT,
    my_sig TEXT,
    my_sig_info TEXT,

    fields TEXT NOT NULL DEFAULT '{}' CHECK (json_valid(fields) AND json_type(fields) = 'object'),

    call_base TEXT NOT NULL,
    qso_time INTEGER NOT NULL,
    freq_hz INTEGER,

    UNIQUE (logbook_id, seq),
    UNIQUE (logbook_id, updated_seq),
    UNIQUE (logbook_id, id),
    FOREIGN KEY (logbook_id, station_id) REFERENCES stations (logbook_id, id),
    FOREIGN KEY (logbook_id, instrument_id) REFERENCES instruments (logbook_id, id)
);

INSERT INTO qsos_new (
    id, logbook_id, station_id, seq, updated_seq, revision, created_by, created_at, updated_by, updated_at,
    deleted_at, call, qso_date, time_on, time_off, band, mode, submode, freq, rst_sent, rst_rcvd,
    station_callsign, operator, name, qth, gridsquare, state, cnty, dxcc, contest_id, stx_string, srx_string,
    comment, tx_pwr, qsl_sent, qsl_sent_via, qslsdate, qsl_rcvd, qsl_rcvd_via, qslrdate, eqsl_qsl_sent,
    eqsl_qslsdate, eqsl_qsl_rcvd, eqsl_qslrdate, lotw_qsl_sent, lotw_qslsdate, lotw_qsl_rcvd, lotw_qslrdate,
    my_gridsquare, my_city, my_state, my_cnty, my_pota_ref, my_sig, my_sig_info, fields, call_base, qso_time,
    freq_hz
)
SELECT
    id, logbook_id, station_id, seq, updated_seq, revision, created_by, created_at, updated_by, updated_at,
    deleted_at, call, qso_date, time_on, time_off, band, mode, submode, freq, rst_sent, rst_rcvd,
    station_callsign, operator, name, qth, gridsquare, state, cnty, dxcc, contest_id, stx_string, srx_string,
    comment, tx_pwr, qsl_sent, qsl_sent_via, qslsdate, qsl_rcvd, qsl_rcvd_via, qslrdate, eqsl_qsl_sent,
    eqsl_qslsdate, eqsl_qsl_rcvd, eqsl_qslrdate, lotw_qsl_sent, lotw_qslsdate, lotw_qsl_rcvd, lotw_qslrdate,
    my_gridsquare, my_city, my_state, my_cnty, my_pota_ref, my_sig, my_sig_info, fields, call_base, qso_time,
    freq_hz
FROM qsos;

CREATE TABLE export_batch_qsos_new (
    logbook_id TEXT NOT NULL,
    batch_id TEXT NOT NULL,
    qso_id TEXT NOT NULL,
    revision INTEGER NOT NULL,
    PRIMARY KEY (batch_id, qso_id),
    FOREIGN KEY (logbook_id, batch_id) REFERENCES export_batches (logbook_id, id) ON DELETE CASCADE,
    FOREIGN KEY (logbook_id, qso_id) REFERENCES qsos_new (logbook_id, id) ON DELETE CASCADE
);

INSERT INTO export_batch_qsos_new (logbook_id, batch_id, qso_id, revision)
SELECT logbook_id, batch_id, qso_id, revision FROM export_batch_qsos;

DROP TABLE export_batch_qsos;
DROP TABLE qsos;
ALTER TABLE qsos_new RENAME TO qsos;
ALTER TABLE export_batch_qsos_new RENAME TO export_batch_qsos;

CREATE INDEX qsos_logbook_time ON qsos (logbook_id, qso_time) WHERE deleted_at IS NULL;
CREATE INDEX qsos_logbook_call ON qsos (logbook_id, call, qso_time) WHERE deleted_at IS NULL;
CREATE INDEX qsos_logbook_call_base ON qsos (logbook_id, call_base, qso_time) WHERE deleted_at IS NULL;
CREATE INDEX qsos_logbook_station ON qsos (logbook_id, station_id, qso_time) WHERE deleted_at IS NULL;
CREATE INDEX qsos_logbook_instrument ON qsos (logbook_id, instrument_id, qso_time) WHERE deleted_at IS NULL;
CREATE INDEX qsos_logbook_qsl_sent ON qsos (logbook_id, qsl_sent) WHERE deleted_at IS NULL;
