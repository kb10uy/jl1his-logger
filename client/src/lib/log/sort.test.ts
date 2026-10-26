import { describe, expect, it } from 'vitest';
import type { Qso } from '#lib/api/types.ts';
import { comparator, directionOf, parseSort, toggled, type Sort, type SortColumn } from './sort.ts';

function qso(id: string, call: string, time: string): Qso {
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
        fields: { CALL: call, QSO_DATE: '20261019', TIME_ON: time }
    };
}

describe('parseSort', () => {
    it.each([
        [null, '-time'],
        ['time', 'time'],
        ['-call', '-call'],
        ['band', '-time']
    ])('reads %s as %s', (value, expected) => {
        expect(parseSort(value)).toBe(expected);
    });
});

describe('toggled', () => {
    it.each<[SortColumn, Sort, Sort]>([
        ['time', '-time', 'time'],
        ['time', 'time', '-time'],
        ['time', 'call', '-time'],
        ['call', '-time', 'call'],
        ['call', 'call', '-call'],
        ['call', '-call', 'call']
    ])('turns %s from %s to %s', (column, sort, expected) => {
        expect(toggled(column, sort)).toBe(expected);
        expect(directionOf(column, expected)).toBe(
            expected.startsWith('-') ? 'descending' : 'ascending'
        );
    });

    it('gives no direction to the other column', () => {
        expect(directionOf('call', '-time')).toBeUndefined();
    });
});

describe('comparator', () => {
    const qsos = [
        qso('a', 'JA1XYZ', '0100'),
        qso('b', 'ja1xyz/p', '0200'),
        qso('c', 'JA1ABC', '0300')
    ];

    it.each<[Sort, string[]]>([
        ['-time', ['c', 'b', 'a']],
        ['time', ['a', 'b', 'c']],
        ['call', ['c', 'b', 'a']],
        ['-call', ['b', 'a', 'c']]
    ])('orders %s with the newest first within a callsign', (sort, expected) => {
        expect([...qsos].sort(comparator(sort)).map((q) => q.id)).toEqual(expected);
    });
});
