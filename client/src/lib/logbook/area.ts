import type { Area, AreaField } from '#lib/api/types.ts';
import { isAdifQsoField } from '#lib/qso/adif-fields.ts';

export interface AreaDraft {
    name: string;
    label: string;
}

export const AREA_PRESETS: readonly AreaField[] = [
    { name: 'CNTY', label: 'County' },
    { name: 'STATE', label: 'State' },
    { name: 'CNTY', label: 'JCC / JCG' },
    { name: 'IOTA', label: 'IOTA' },
    { name: 'SOTA_REF', label: 'SOTA' },
    { name: 'POTA_REF', label: 'POTA' }
];

const NOT_AREA = new Set([
    'CALL',
    'QSO_DATE',
    'TIME_ON',
    'BAND',
    'MODE',
    'SUBMODE',
    'FREQ',
    'RST_SENT',
    'RST_RCVD',
    'NAME',
    'QTH',
    'COMMENT',
    'NOTES',
    'TX_PWR',
    'STATION_CALLSIGN',
    'OPERATOR'
]);

export function areaDraftOf(area: Area): AreaDraft {
    return { name: area?.name ?? '', label: area?.label ?? '' };
}

export function areaInput(draft: AreaDraft): Area {
    const name = draft.name.trim().toUpperCase();
    const label = draft.label.trim();
    return name === '' && label === '' ? null : { name, label };
}

export function areaProblem(draft: AreaDraft): string | undefined {
    const input = areaInput(draft);
    if (input === null) return undefined;
    if (input.name === '') return 'Name the ADIF field, or clear the label.';
    if (!isAdifQsoField(input.name)) return `${input.name} is not an ADIF QSO field.`;
    if (NOT_AREA.has(input.name) || input.name.startsWith('MY_') || input.name.startsWith('QSL'))
        return `${input.name} has its own place on the screens.`;
    if (input.label === '') return 'Give the field a label.';
    return undefined;
}
