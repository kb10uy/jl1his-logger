import { describe, expect, it } from 'vitest';
import type { Qso } from '#lib/api/types.ts';
import { applyEvent, distribution, replaceRows, undoOf, type Row } from './rows.ts';

function qso(id: string, time: string, patch: Partial<Qso> = {}, fields = {}): Qso {
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
        fields: { CALL: 'JA1XYZ', QSO_DATE: '20261019', TIME_ON: time, ...fields },
        ...patch
    };
}

const rows = (...qsos: Qso[]): Row[] => qsos.map((q) => ({ qso: q, stale: false }));
const ids = (applied: ReturnType<typeof applyEvent>) =>
    'rows' in applied ? applied.rows.map((r) => [r.qso.id, r.stale]) : applied.kind;

describe('applyEvent', () => {
    const loaded = rows(qso('c', '0930'), qso('a', '0900'));

    it('updates a loaded row in place and marks it when it stops matching', () => {
        expect(ids(applyEvent(loaded, qso('a', '0900', { updated_seq: 5 }), true, false))).toEqual([
            ['c', false],
            ['a', false]
        ]);
        expect(ids(applyEvent(loaded, qso('c', '0930', { updated_seq: 5 }), false, false))).toEqual(
            [
                ['c', true],
                ['a', false]
            ]
        );
        const deleted = qso('a', '0900', { updated_seq: 5, deleted_at: 'x' });
        expect(ids(applyEvent(loaded, deleted, true, false))).toEqual([
            ['c', false],
            ['a', true]
        ]);
    });

    it('ignores events it has seen', () => {
        expect(applyEvent(loaded, qso('a', '0900'), true, false)).toEqual({ kind: 'ignored' });
    });

    it('inserts a new matching row in order', () => {
        expect(ids(applyEvent(loaded, qso('b', '0915'), true, false))).toEqual([
            ['c', false],
            ['b', false],
            ['a', false]
        ]);
        expect(ids(applyEvent(loaded, qso('d', '1000'), true, false))[0]).toEqual(['d', false]);
    });

    it('leaves older rows to paging unless the list is complete', () => {
        expect(applyEvent(loaded, qso('z', '0800'), true, false)).toEqual({ kind: 'ignored' });
        expect(ids(applyEvent(loaded, qso('z', '0800'), true, true)).at(-1)).toEqual(['z', false]);
    });

    it('counts only new rows outside the query', () => {
        expect(applyEvent(loaded, qso('n', '1000'), false, false)).toEqual({ kind: 'outside' });
        expect(applyEvent(loaded, qso('n', '1000', { revision: 2 }), false, false)).toEqual({
            kind: 'ignored'
        });
    });
});

describe('bulk changes', () => {
    const before = [
        qso('a', '0900', {}, { QSL_SENT: 'R' }),
        qso('b', '0910', {}, { QSL_SENT: 'R' }),
        qso('c', '0920')
    ];
    const after = before.map((q) => ({
        ...q,
        revision: 2,
        fields: { ...q.fields, QSL_SENT: 'Y', QSLSDATE: '20261023' }
    }));

    it('groups the undo by the values each row had', () => {
        expect(undoOf(before, after, ['QSL_SENT', 'QSLSDATE'])).toEqual([
            {
                qsos: [
                    { id: 'a', revision: 2 },
                    { id: 'b', revision: 2 }
                ],
                fields: { QSL_SENT: 'R', QSLSDATE: '' }
            },
            { qsos: [{ id: 'c', revision: 2 }], fields: { QSL_SENT: '', QSLSDATE: '' } }
        ]);
    });

    it('cannot undo rows it never saw', () => {
        expect(undoOf(before.slice(1), after, ['QSL_SENT'])).toBeUndefined();
    });

    it('replaces updated rows and counts values', () => {
        const replaced = replaceRows(rows(...before), [after[1]], (q) => q.fields.QSL_SENT === 'R');
        expect(replaced.map((r) => [r.qso.revision, r.stale])).toEqual([
            [1, false],
            [2, true],
            [1, false]
        ]);
        expect(distribution(before, 'QSL_SENT')).toBe('R ×2 · — ×1');
    });
});
