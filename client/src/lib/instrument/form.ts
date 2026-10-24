import type { Fields, Instrument, InstrumentInput } from '#lib/api/types.ts';
import { m } from '#lib/paraglide/messages.js';
import { ADIF_QSO_FIELDS, isAdifQsoField } from '#lib/qso/adif-fields.ts';
import { isPower, powerRule } from '#lib/qso/power.ts';

const RESERVED = new Set([
    'CALL',
    'QSO_DATE',
    'TIME_ON',
    'BAND',
    'MODE',
    'STATION_CALLSIGN',
    'MY_RIG',
    'MY_ANTENNA',
    'TX_PWR'
]);

export const FIELD_NAMES = ADIF_QSO_FIELDS.filter((name) => !RESERVED.has(name));

export interface InstrumentDraft {
    name: string;
    rig: string;
    antenna: string;
    power: string;
    extra: { name: string; value: string }[];
}

export function draftOf(instrument?: Instrument): InstrumentDraft {
    return {
        name: instrument?.name ?? '',
        rig: instrument?.my_rig ?? '',
        antenna: instrument?.my_antenna ?? '',
        power: instrument?.tx_pwr ?? '',
        extra: Object.entries(instrument?.fields ?? {}).map(([name, value]) => ({ name, value }))
    };
}

const optional = (value: string) => (value.trim() === '' ? null : value.trim());

export function instrumentInput(draft: InstrumentDraft): InstrumentInput {
    const fields: Fields = {};
    for (const { name, value } of draft.extra) {
        const key = name.trim().toUpperCase();
        if (key !== '' && value.trim() !== '') fields[key] = value.trim();
    }
    return {
        name: draft.name.trim(),
        my_rig: optional(draft.rig),
        my_antenna: optional(draft.antenna),
        tx_pwr: optional(draft.power),
        fields
    };
}

export function instrumentProblem(draft: InstrumentDraft): string | undefined {
    if (draft.name.trim() === '') return m.stations_instrument_problem_name();
    const power = draft.power.trim();
    if (power !== '' && !isPower(power)) return powerRule();
    const names = new Set<string>();
    for (const { name } of draft.extra) {
        const key = name.trim().toUpperCase();
        if (key === '') continue;
        if (!isAdifQsoField(key)) return m.stations_problem_not_adif({ name });
        if (RESERVED.has(key)) return m.stations_instrument_problem_reserved({ key });
        if (names.has(key)) return m.stations_problem_twice({ key });
        names.add(key);
    }
    return undefined;
}

export function sameInput(a: InstrumentInput, b: InstrumentInput): boolean {
    const sorted = (f: Fields) => Object.entries(f).sort(([x], [y]) => (x < y ? -1 : 1));
    return (
        a.name === b.name &&
        a.my_rig === b.my_rig &&
        a.my_antenna === b.my_antenna &&
        a.tx_pwr === b.tx_pwr &&
        JSON.stringify(sorted(a.fields)) === JSON.stringify(sorted(b.fields))
    );
}
