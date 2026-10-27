export type Fields = Record<string, string>;

export interface Me {
    id: string;
    callsign: string;
    role: 'admin' | 'user';
}

export type LogbookRole = 'owner' | 'operator';

export interface AreaField {
    name: string;
    label: string;
}

export type Area = AreaField | null;

export interface Logbook {
    id: string;
    name: string;
    role: LogbookRole;
    cursor_generation: string;
    last_seq: number;
    last_updated_seq: number;
    area_field: Area;
}

export interface Station {
    id: string;
    wavelog_id: number | null;
    name: string;
    callsign: string;
    fields: Fields;
    archived_at: string | null;
    created_at: string;
    updated_at: string;
    favorite: boolean;
}

export interface Instrument {
    id: string;
    name: string;
    my_rig: string | null;
    my_antenna: string | null;
    tx_pwr: string | null;
    fields: Fields;
    archived_at: string | null;
    created_at: string;
    updated_at: string;
    favorite: boolean;
}

export interface Qso {
    id: string;
    station_id: string;
    instrument_id: string | null;
    seq: number;
    updated_seq: number;
    revision: number;
    created_by: string;
    created_by_callsign: string | null;
    created_at: string;
    updated_by: string;
    updated_by_callsign: string | null;
    updated_at: string;
    deleted_at: string | null;
    fields: Fields;
}

export type Warning =
    | { kind: 'possible_duplicate'; ids: string[] }
    | { kind: 'frequency_outside_band'; freq: string; band: string }
    | { kind: 'unknown_mode'; mode: string }
    | { kind: 'unknown_submode'; submode: string }
    | { kind: 'submode_of_another_mode'; mode: string; submode: string; expected: string };

export interface Saved {
    qso: Qso;
    warnings: Warning[];
}

export interface Page {
    qsos: Qso[];
    next: string | null;
    total?: number;
}

export interface Target {
    id: string;
    revision: number;
}

export interface Skipped {
    id: string;
    reason: 'missing' | 'deleted' | 'revision';
    current_revision?: number;
}

export interface Outcome {
    updated: Qso[];
    skipped: Skipped[];
}

export interface Entity {
    name: string;
    dxcc: number | null;
    continent: string;
    cq_zone: number;
    itu_zone: number;
    latitude: number;
    longitude: number;
}

export interface Lookup {
    call: string;
    entity: Entity | null;
    previous: {
        count: number;
        last_qso_at: string | null;
        fields: Fields;
    };
}

export interface Hello {
    cursor_generation: string;
    last_updated_seq: number;
    after: number;
}

export interface QsoEvent {
    cursor_generation: string;
    qso: Qso;
}

export interface StationInput {
    name: string;
    callsign: string;
    wavelog_id: number | null;
    fields: Fields;
}

export interface InstrumentInput {
    name: string;
    my_rig: string | null;
    my_antenna: string | null;
    tx_pwr: string | null;
    fields: Fields;
}

export interface Member {
    user_id: string;
    callsign: string;
    role: LogbookRole;
    disabled: boolean;
}

export interface Passkey {
    id: string;
    name: string;
    created_at: string;
    last_used_at: string | null;
}

export type TokenKind = 'native' | 'wl2';

export interface Token {
    id: string;
    user_id: string;
    kind: TokenKind;
    name: string;
    logbook_ids: string[];
    created_at: string;
    last_used_at: string | null;
}

export interface ServerVersion {
    version: string;
    commit: string | null;
    adif: string;
}

export type ReferenceKind = 'cty' | 'parks';

export interface ReferenceVersion {
    kind: ReferenceKind;
    version: string;
    updated_at: string;
}

export interface Subdivision {
    code: string;
    name: string;
}

export type ExportKind = 'adif' | 'pota' | 'eqsl' | 'lotw' | 'qsl_card';

export type LengthUnit = 'codepoints' | 'bytes' | 'utf16';

export type Encoding = 'utf-8' | 'shift_jis' | 'windows-1252' | 'ascii';

export type Unencodable = 'error' | 'replace' | 'entity' | 'omit';

export interface Batch {
    id: string;
    kind: ExportKind;
    created_by: string;
    created_at: string;
    applied_at: string | null;
    qso_count: number;
}

export interface Exported {
    batch: Batch | null;
    records: number;
    adif: string | null;
    file: string | null;
}

export type ImportSource = 'new' | 'lotw' | 'eqsl' | 'card';

export interface RecordIds {
    record: number;
    ids: string[];
}

export interface Invalid {
    record: number;
    error: string;
}

export interface NewReport {
    applied: boolean;
    encoding: Encoding;
    lengths: LengthUnit[];
    read: number;
    created: number;
    existing: number;
    duplicates: RecordIds[];
    outside_band: { record: number; freq: string; band: string }[];
    modes: ({ record: number } & Warning)[];
    invalid: Invalid[];
    rejects: string | null;
}

export interface MergeReport {
    applied: boolean;
    encoding: Encoding;
    lengths: LengthUnit[];
    read: number;
    not_confirmations: number;
    merged: number;
    already: number;
    unmatched: number[];
    ambiguous: RecordIds[];
    invalid: Invalid[];
    skipped: Skipped[];
    differences: Record<string, number>;
    rejects: string | null;
}
