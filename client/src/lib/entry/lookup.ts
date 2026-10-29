import type { Candidate } from '#lib/api/api.ts';
import { utcFields } from '#lib/qso/format.ts';

export const LOOKUP_DELAY = 500;

export function candidateOf(band: string, mode: string, at: Date): Candidate | undefined {
    if (band === '' || mode === '') return undefined;
    const { QSO_DATE, TIME_ON } = utcFields(at);
    return { band, mode, qso_date: QSO_DATE, time_on: TIME_ON };
}
