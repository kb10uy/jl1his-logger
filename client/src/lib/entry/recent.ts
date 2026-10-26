import type { Qso } from '#lib/api/types.ts';
import { compareNewestFirst } from '#lib/qso/format.ts';

export const RECENT = 20;

export function mergeRecent(recent: Qso[], qsos: Qso[], stationId: string): Qso[] {
    const incoming = qsos.filter(
        (q) => !recent.some((r) => r.id === q.id && r.updated_seq > q.updated_seq)
    );
    const ids = new Set(incoming.map((q) => q.id));
    return [
        ...recent.filter((r) => !ids.has(r.id)),
        ...incoming.filter((q) => q.deleted_at === null && q.station_id === stationId)
    ]
        .sort(compareNewestFirst)
        .slice(0, RECENT);
}
