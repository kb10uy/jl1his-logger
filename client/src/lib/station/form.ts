import type { Fields, Station, StationInput } from '#lib/api/types.ts';
import { ADIF_QSO_FIELDS, isAdifQsoField } from '#lib/qso/adif-fields.ts';

export const MAIN_FIELDS = [
    { name: 'MY_GRIDSQUARE', label: 'Grid locator' },
    { name: 'MY_CITY', label: 'City' },
    { name: 'MY_STATE', label: 'State' },
    { name: 'MY_CNTY', label: 'County' },
    { name: 'MY_DXCC', label: 'DXCC entity' }
] as const;

export const EXTRA_FIELDS = [
    { name: 'MY_SIG', label: 'Special activity' },
    { name: 'MY_SIG_INFO', label: 'Special activity info' },
    { name: 'MY_IOTA', label: 'IOTA' },
    { name: 'MY_WWFF_REF', label: 'WWFF' },
    { name: 'MY_CQ_ZONE', label: 'CQ zone' },
    { name: 'MY_ITU_ZONE', label: 'ITU zone' },
    { name: 'OPERATOR', label: 'Operator' }
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
    if (draft.name.trim() === '') return 'A Station needs a name.';
    if (draft.callsign.trim() === '') return 'A Station needs a callsign.';
    if (draft.wavelogId.trim() !== '' && !/^[1-9]\d*$/.test(draft.wavelogId.trim()))
        return 'The Wavelog ID is a positive whole number.';
    const dxcc = draft.main.MY_DXCC?.trim() ?? '';
    if (dxcc !== '' && !/^\d+$/.test(dxcc)) return 'MY_DXCC is a DXCC entity code.';
    const names = new Set<string>();
    for (const { name } of draft.extra) {
        const key = name.trim().toUpperCase();
        if (key === '') continue;
        if (!isAdifQsoField(key)) return `${name} is not an ADIF QSO field.`;
        if (RESERVED.has(key)) return `${key} cannot be a Station default.`;
        if (EQUIPMENT.has(key)) return `${key} belongs to an Instrument.`;
        if (MAIN.has(key) || names.has(key)) return `${key} is listed twice.`;
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
