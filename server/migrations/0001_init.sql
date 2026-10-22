CREATE TABLE settings (
    key TEXT PRIMARY KEY NOT NULL,
    value TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE users (
    id TEXT PRIMARY KEY NOT NULL,
    callsign TEXT NOT NULL UNIQUE,
    role TEXT NOT NULL CHECK (role IN ('admin', 'user')),
    disabled_at TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE passkeys (
    id TEXT PRIMARY KEY NOT NULL,
    user_id TEXT NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    credential_id BLOB NOT NULL UNIQUE,
    passkey TEXT NOT NULL,
    name TEXT NOT NULL,
    created_at TEXT NOT NULL,
    last_used_at TEXT
);

CREATE INDEX passkeys_user_id ON passkeys (user_id);

CREATE TABLE registration_tokens (
    token_hash BLOB PRIMARY KEY NOT NULL,
    user_id TEXT NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    created_by TEXT REFERENCES users (id) ON DELETE SET NULL,
    created_at TEXT NOT NULL,
    expires_at TEXT NOT NULL,
    used_at TEXT
);

CREATE TABLE sessions (
    token_hash BLOB PRIMARY KEY NOT NULL,
    user_id TEXT NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    created_at TEXT NOT NULL,
    expires_at TEXT NOT NULL
);

CREATE INDEX sessions_user_id ON sessions (user_id);

CREATE TABLE logbooks (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    last_seq INTEGER NOT NULL DEFAULT 0,
    last_updated_seq INTEGER NOT NULL DEFAULT 0,
    cursor_generation TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE memberships (
    logbook_id TEXT NOT NULL REFERENCES logbooks (id) ON DELETE CASCADE,
    user_id TEXT NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    role TEXT NOT NULL CHECK (role IN ('owner', 'operator')),
    created_at TEXT NOT NULL,
    PRIMARY KEY (logbook_id, user_id)
);

CREATE INDEX memberships_user_id ON memberships (user_id);

CREATE TABLE stations (
    id TEXT PRIMARY KEY NOT NULL,
    logbook_id TEXT NOT NULL REFERENCES logbooks (id) ON DELETE CASCADE,
    wavelog_id INTEGER,
    name TEXT NOT NULL,
    callsign TEXT NOT NULL,
    fields TEXT NOT NULL DEFAULT '{}' CHECK (json_valid(fields) AND json_type(fields) = 'object'),
    archived_at TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    UNIQUE (logbook_id, wavelog_id),
    UNIQUE (logbook_id, id)
);

CREATE INDEX stations_logbook_id ON stations (logbook_id);

CREATE TABLE qsos (
    id TEXT PRIMARY KEY NOT NULL,
    logbook_id TEXT NOT NULL REFERENCES logbooks (id) ON DELETE CASCADE,
    station_id TEXT NOT NULL,
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
    FOREIGN KEY (logbook_id, station_id) REFERENCES stations (logbook_id, id)
);

CREATE INDEX qsos_logbook_time ON qsos (logbook_id, qso_time) WHERE deleted_at IS NULL;
CREATE INDEX qsos_logbook_call ON qsos (logbook_id, call, qso_time) WHERE deleted_at IS NULL;
CREATE INDEX qsos_logbook_call_base ON qsos (logbook_id, call_base, qso_time) WHERE deleted_at IS NULL;
CREATE INDEX qsos_logbook_station ON qsos (logbook_id, station_id, qso_time) WHERE deleted_at IS NULL;
CREATE INDEX qsos_logbook_qsl_sent ON qsos (logbook_id, qsl_sent) WHERE deleted_at IS NULL;

CREATE TABLE api_tokens (
    id TEXT PRIMARY KEY NOT NULL,
    user_id TEXT NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    kind TEXT NOT NULL CHECK (kind IN ('native', 'wl2')),
    name TEXT NOT NULL,
    token_hash BLOB NOT NULL UNIQUE,
    created_at TEXT NOT NULL,
    last_used_at TEXT
);

CREATE INDEX api_tokens_user_id ON api_tokens (user_id);

CREATE TABLE api_token_scopes (
    token_id TEXT NOT NULL REFERENCES api_tokens (id) ON DELETE CASCADE,
    logbook_id TEXT NOT NULL REFERENCES logbooks (id) ON DELETE CASCADE,
    PRIMARY KEY (token_id, logbook_id)
);

CREATE TABLE export_batches (
    id TEXT PRIMARY KEY NOT NULL,
    logbook_id TEXT NOT NULL REFERENCES logbooks (id) ON DELETE CASCADE,
    kind TEXT NOT NULL,
    created_by TEXT NOT NULL REFERENCES users (id),
    created_at TEXT NOT NULL,
    applied_at TEXT,
    UNIQUE (logbook_id, id)
);

CREATE INDEX export_batches_logbook_id ON export_batches (logbook_id, created_at);

CREATE TABLE export_batch_qsos (
    logbook_id TEXT NOT NULL,
    batch_id TEXT NOT NULL,
    qso_id TEXT NOT NULL,
    revision INTEGER NOT NULL,
    PRIMARY KEY (batch_id, qso_id),
    FOREIGN KEY (logbook_id, batch_id) REFERENCES export_batches (logbook_id, id) ON DELETE CASCADE,
    FOREIGN KEY (logbook_id, qso_id) REFERENCES qsos (logbook_id, id) ON DELETE CASCADE
);

CREATE TABLE rescue_operations (
    id TEXT PRIMARY KEY NOT NULL,
    logbook_id TEXT NOT NULL REFERENCES logbooks (id) ON DELETE CASCADE,
    actor_id TEXT NOT NULL REFERENCES users (id),
    action TEXT NOT NULL,
    reason TEXT NOT NULL,
    created_at TEXT NOT NULL
);

CREATE TABLE reference_versions (
    kind TEXT PRIMARY KEY NOT NULL,
    version TEXT NOT NULL,
    checksum BLOB,
    updated_at TEXT NOT NULL
);
