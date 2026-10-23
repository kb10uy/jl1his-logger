export type Fields = Record<string, string>;

export interface Me {
    id: string;
    callsign: string;
    role: 'admin' | 'user';
}

export type LogbookRole = 'owner' | 'operator';

export interface Logbook {
    id: string;
    name: string;
    role: LogbookRole;
    cursor_generation: string;
    last_seq: number;
    last_updated_seq: number;
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
}

export interface Qso {
    id: string;
    station_id: string;
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
