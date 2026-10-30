import type { Station } from '#client/api/types.ts';
import { splitFavorites } from '#client/favorite.ts';
import type { Logbook } from '#lib/session/types.ts';

export interface StationGroup {
    logbook: Logbook;
    stations: Station[];
}

export interface Choice {
    logbookId: string;
    stationId: string;
}

export function stationGroups(
    logbooks: Logbook[],
    stations: Readonly<Record<string, Station[]>>
): StationGroup[] {
    return logbooks
        .map((logbook) => {
            const active = (stations[logbook.id] ?? []).filter((s) => s.archived_at === null);
            const { favorites, others } = splitFavorites(active);
            return { logbook, stations: [...favorites, ...others] };
        })
        .filter((group) => group.stations.length > 0);
}

export function chooseStation(groups: StationGroup[], wanted: string): Choice | undefined {
    const found = groups.find((g) => g.stations.some((s) => s.id === wanted));
    if (found !== undefined) return { logbookId: found.logbook.id, stationId: wanted };
    const first = groups[0];
    return first && { logbookId: first.logbook.id, stationId: first.stations[0].id };
}
