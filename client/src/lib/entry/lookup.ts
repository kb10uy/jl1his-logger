import type { Candidate } from '#lib/api/api.ts';
import type { Lookup } from '#lib/api/types.ts';
import { utcFields } from '#lib/qso/format.ts';

export const LOOKUP_DELAY = 500;

export function candidateOf(band: string, mode: string, at: Date): Candidate | undefined {
    if (band === '' || mode === '') return undefined;
    const { QSO_DATE, TIME_ON } = utcFields(at);
    return { band, mode, qso_date: QSO_DATE, time_on: TIME_ON };
}

export function lookupLine(lookup: Lookup): string {
    const entity = lookup.entity;
    return [
        entity?.name,
        entity?.dxcc ? `DXCC ${entity.dxcc}` : undefined,
        entity ? `CQ ${entity.cq_zone}` : undefined,
        entity ? `ITU ${entity.itu_zone}` : undefined,
        lookup.previous.fields.NAME,
        lookup.previous.fields.QTH
    ]
        .filter((s) => s)
        .join(' · ');
}
