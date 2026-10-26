import { describe, expect, it } from 'vitest';
import type { Qso } from '#lib/api/types.ts';
import { RECENT, mergeRecent } from './recent.ts';

function qso(id: string, time: string, patch: Partial<Qso> = {}): Qso {
    return {
        id,
        station_id: 'st',
        instrument_id: null,
        seq: 1,
        updated_seq: 1,
        revision: 1,
        created_by: 'u',
        created_by_callsign: 'JL1HIS',
        created_at: '',
        updated_by: 'u',
        updated_by_callsign: 'JL1HIS',
        updated_at: '',
        deleted_at: null,
        fields: { CALL: 'JA1XYZ', QSO_DATE: '20261019', TIME_ON: time },
        ...patch
    };
}

const ids = (qsos: Qso[]) => qsos.map((q) => q.id);

describe('mergeRecent', () => {
    const recent = [qso('b', '0930'), qso('a', '0900')];

    it('inserts in newest-first order', () => {
        expect(ids(mergeRecent(recent, [qso('c', '0915')], 'st'))).toEqual(['b', 'c', 'a']);
    });

    it('replaces a QSO it already has', () => {
        const merged = mergeRecent(recent, [qso('a', '1000', { updated_seq: 2 })], 'st');
        expect(ids(merged)).toEqual(['a', 'b']);
    });

    it('keeps the newer copy', () => {
        const held = [qso('a', '0900', { updated_seq: 5 })];
        const merged = mergeRecent(held, [qso('a', '1000', { updated_seq: 4 })], 'st');
        expect(merged[0].updated_seq).toBe(5);
    });

    it('drops deleted QSOs and QSOs moved to another Station', () => {
        const deleted = qso('a', '0900', { updated_seq: 2, deleted_at: '2026-10-19T10:00:00Z' });
        const moved = qso('b', '0930', { updated_seq: 2, station_id: 'other' });
        expect(ids(mergeRecent(recent, [deleted, moved], 'st'))).toEqual([]);
    });

    it('ignores QSOs of another Station', () => {
        const other = qso('c', '1000', { station_id: 'other' });
        expect(ids(mergeRecent(recent, [other], 'st'))).toEqual(['b', 'a']);
    });

    it('keeps only the newest ones', () => {
        const many = Array.from({ length: RECENT + 5 }, (_, i) => qso(`q${i}`, String(1000 + i)));
        const merged = mergeRecent([], many, 'st');
        expect(merged).toHaveLength(RECENT);
        expect(merged[0].id).toBe(`q${RECENT + 4}`);
    });
});
