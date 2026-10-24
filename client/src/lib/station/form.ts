import type { Fields, Station, StationInput } from '#lib/api/types.ts';
import { m } from '#lib/paraglide/messages.js';
import { ADIF_QSO_FIELDS, isAdifQsoField } from '#lib/qso/adif-fields.ts';

export const MAIN_FIELDS = [
    { name: 'MY_GRIDSQUARE', label: m.stations_field_grid },
    { name: 'MY_CITY', label: m.stations_field_city },
    { name: 'MY_STATE', label: m.stations_field_state },
    { name: 'MY_CNTY', label: m.stations_field_county },
    { name: 'MY_DXCC', label: m.stations_field_dxcc }
] as const;

export const EXTRA_FIELDS = [
    { name: 'MY_SIG', label: m.stations_field_sig },
    { name: 'MY_SIG_INFO', label: m.stations_field_sig_info },
    { name: 'MY_IOTA', label: m.stations_field_iota },
    { name: 'MY_WWFF_REF', label: m.stations_field_wwff },
    { name: 'MY_CQ_ZONE', label: m.stations_field_cq_zone },
    { name: 'MY_ITU_ZONE', label: m.stations_field_itu_zone },
    { name: 'OPERATOR', label: m.stations_field_operator }
] as const;

const RESERVED = new Set(['CALL', 'QSO_DATE', 'TIME_ON', 'BAND', 'MODE', 'STATION_CALLSIGN']);
const EQUIPMENT = new Set(['MY_RIG', 'MY_ANTENNA', 'TX_PWR']);
const MAIN = new Set<string>([...MAIN_FIELDS.map((f) => f.name), 'MY_POTA_REF']);

export const FIELD_NAMES = ADIF_QSO_FIELDS.filter(
    (name) => !RESERVED.has(name) && !EQUIPMENT.has(name) && !MAIN.has(name)
);

export interface StationDraft {
    name: string;
    callsign: string;
    wavelogId: string;
    main: Record<string, string>;
    parks: string[];
    extra: { name: string; value: string }[];
}

export function splitParks(value: string | undefined): string[] {
    return [
        ...new Set(
            (value ?? '')
                .split(/[\s,]+/)
                .map((p) => p.trim().toUpperCase())
                .filter((p) => p !== '')
        )
    ];
}

export function draftOf(station?: Station): StationDraft {
    const fields = station?.fields ?? {};
    return {
        name: station?.name ?? '',
        callsign: station?.callsign ?? '',
        wavelogId: station?.wavelog_id == null ? '' : String(station.wavelog_id),
        main: Object.fromEntries(MAIN_FIELDS.map((f) => [f.name, fields[f.name] ?? ''])),
        parks: splitParks(fields.MY_POTA_REF),
        extra: Object.entries(fields)
            .filter(([name]) => !MAIN.has(name))
            .map(([name, value]) => ({ name, value }))
    };
}

export function stationInput(draft: StationDraft): StationInput {
    const fields: Fields = {};
    for (const [name, value] of Object.entries(draft.main)) {
        if (value.trim() !== '') fields[name] = value.trim();
    }
    if (draft.parks.length > 0) fields.MY_POTA_REF = draft.parks.join(',');
    for (const { name, value } of draft.extra) {
        const key = name.trim().toUpperCase();
        if (key !== '' && value.trim() !== '') fields[key] = value.trim();
    }
    const id = draft.wavelogId.trim();
    return {
        name: draft.name.trim(),
        callsign: draft.callsign.trim().toUpperCase(),
        wavelog_id: id === '' ? null : Number(id),
        fields
    };
}

export function stationProblem(draft: StationDraft): string | undefined {
    if (draft.name.trim() === '') return m.stations_problem_name();
    if (draft.callsign.trim() === '') return m.stations_problem_callsign();
    if (draft.wavelogId.trim() !== '' && !/^[1-9]\d*$/.test(draft.wavelogId.trim()))
        return m.stations_problem_wavelog_id();
    const dxcc = draft.main.MY_DXCC?.trim() ?? '';
    if (dxcc !== '' && !/^\d+$/.test(dxcc)) return m.stations_problem_dxcc();
    const names = new Set<string>();
    for (const { name } of draft.extra) {
        const key = name.trim().toUpperCase();
        if (key === '') continue;
        if (!isAdifQsoField(key)) return m.stations_problem_not_adif({ name });
        if (RESERVED.has(key)) return m.stations_problem_reserved({ key });
        if (EQUIPMENT.has(key)) return m.stations_problem_equipment({ key });
        if (MAIN.has(key) || names.has(key)) return m.stations_problem_twice({ key });
        names.add(key);
    }
    return undefined;
}

export function sameInput(a: StationInput, b: StationInput): boolean {
    const sorted = (f: Fields) => Object.entries(f).sort(([x], [y]) => (x < y ? -1 : 1));
    return (
        a.name === b.name &&
        a.callsign === b.callsign &&
        a.wavelog_id === b.wavelog_id &&
        JSON.stringify(sorted(a.fields)) === JSON.stringify(sorted(b.fields))
    );
}
