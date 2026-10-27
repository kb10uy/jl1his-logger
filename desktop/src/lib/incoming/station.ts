import type { Fields, Station } from '#client/api/types.ts';

export interface Mismatch {
    name: string;
    given: string;
    station: string;
}

export function stationMismatches(
    fields: Fields,
    station: Pick<Station, 'callsign' | 'fields'>
): Mismatch[] {
    const own: Fields = { ...station.fields, STATION_CALLSIGN: station.callsign };
    return Object.entries(own).flatMap(([name, value]) => {
        const given = (fields[name] ?? '').trim();
        const kept = value.trim();
        if (given === '' || kept === '') return [];
        const a = given.toUpperCase();
        const b = kept.toUpperCase();
        if (a === b || (name === 'MY_GRIDSQUARE' && b.startsWith(a))) return [];
        return [{ name, given, station: kept }];
    });
}
