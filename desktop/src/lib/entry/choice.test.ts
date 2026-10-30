import { describe, expect, it } from 'vitest';
import type { Station } from '#client/api/types.ts';
import type { Logbook } from '#lib/session/types.ts';
import { chooseStation, stationGroups } from './choice.ts';

const logbook = (id: string): Logbook => ({ id, name: id, role: 'owner', area_field: null });

const station = (id: string, extra: Partial<Station> = {}): Station => ({
    id,
    name: id,
    callsign: 'JL1HIS',
    fields: {},
    archived_at: null,
    created_at: '',
    updated_at: '',
    favorite: false,
    ...extra
});

describe('stationGroups', () => {
    it('keeps active Stations per Logbook, favorites first', () => {
        const groups = stationGroups([logbook('main'), logbook('club'), logbook('empty')], {
            main: [station('home'), station('park', { favorite: true })],
            club: [station('old', { archived_at: '2026-01-01T00:00:00Z' }), station('club')]
        });
        expect(groups.map((g) => [g.logbook.id, g.stations.map((s) => s.id)])).toEqual([
            ['main', ['park', 'home']],
            ['club', ['club']]
        ]);
    });
});

describe('chooseStation', () => {
    const groups = stationGroups([logbook('main'), logbook('club')], {
        main: [station('home')],
        club: [station('club')]
    });

    it('finds the Logbook of the wanted Station', () => {
        expect(chooseStation(groups, 'club')).toEqual({ logbookId: 'club', stationId: 'club' });
    });

    it('falls back to the first Station', () => {
        expect(chooseStation(groups, 'gone')).toEqual({ logbookId: 'main', stationId: 'home' });
        expect(chooseStation([], 'gone')).toBeUndefined();
    });
});
