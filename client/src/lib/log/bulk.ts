import type { BulkChange, Fields } from '#lib/api/types.ts';

export interface FieldEdit {
    name: string;
    value: string;
    clear: boolean;
}

export type FieldProblem = 'name' | 'repeated';

export const KEEP = '';
export const NO_INSTRUMENT = '-';

const NAME = /^[A-Z][A-Z0-9_]*$/;

const normalized = (name: string) => name.trim().toUpperCase();

export function fieldProblems(fixed: Fields, edits: FieldEdit[]): (FieldProblem | undefined)[] {
    const seen = new Set(Object.keys(fixed));
    return edits.map((edit) => {
        const name = normalized(edit.name);
        if (name === '') return undefined;
        if (!NAME.test(name) || name === 'EOR' || name === 'EOH') return 'name';
        if (seen.has(name)) return 'repeated';
        seen.add(name);
        return undefined;
    });
}

export function bulkChange(
    fixed: Fields,
    edits: FieldEdit[],
    station: string,
    instrument: string
): BulkChange {
    const fields: Fields = { ...fixed };
    for (const edit of edits) {
        const name = normalized(edit.name);
        if (name === '' || (!edit.clear && edit.value === '')) continue;
        fields[name] = edit.clear ? '' : edit.value;
    }
    const change: BulkChange = { fields };
    if (station !== KEEP) change.station_id = station;
    if (instrument !== KEEP)
        change.instrument_id = instrument === NO_INSTRUMENT ? null : instrument;
    return change;
}

export const isEmpty = (change: BulkChange) =>
    change.station_id === undefined &&
    change.instrument_id === undefined &&
    Object.keys(change.fields ?? {}).length === 0;
