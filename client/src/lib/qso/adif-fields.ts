import { ADIF_QSO_FIELDS } from '#lib/adif/tables.ts';

export { ADIF_QSO_FIELDS };

const KNOWN = new Set(ADIF_QSO_FIELDS);

export function isAdifQsoField(name: string): boolean {
    return KNOWN.has(name) || /^APP_[A-Z0-9_]+$/.test(name);
}
