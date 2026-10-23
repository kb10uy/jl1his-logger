import { describe, expect, it } from 'vitest';
import {
    DEFAULT_COLUMNS,
    adifColumnProblem,
    column,
    moveColumn,
    normalizeColumns
} from './columns.ts';

describe('column', () => {
    it('knows the built-in columns and ADIF fields', () => {
        expect(column('time')?.label).toBe('Date / UTC');
        expect(column('freq')?.right).toBe(true);
        expect(column('lotw')?.label).toBe('LoTW S / R');
        expect(column('adif.TX_PWR')).toEqual({ id: 'adif.TX_PWR', label: 'TX_PWR' });
        expect(column('adif.APP_JL1HIS_X')?.label).toBe('APP_JL1HIS_X');
    });

    it('rejects unknown columns', () => {
        expect(column('date')).toBeUndefined();
        expect(column('adif.NOPE')).toBeUndefined();
        expect(column('adif.tx_pwr')).toBeUndefined();
    });
});

describe('adifColumnProblem', () => {
    it('accepts ADIF QSO fields and APP_ fields', () => {
        expect(adifColumnProblem('NOTES')).toBeUndefined();
        expect(adifColumnProblem('APP_X')).toBeUndefined();
    });

    it('explains what is wrong', () => {
        expect(adifColumnProblem('')).toBe('Enter an ADIF field name.');
        expect(adifColumnProblem('FOO')).toBe('FOO is not an ADIF QSO field.');
    });
});

describe('normalizeColumns', () => {
    it('keeps the stored order and drops unknown or repeated columns', () => {
        expect(normalizeColumns(['call', 'nope', 'adif.NAME', 'call', 3, 'time'])).toEqual([
            'call',
            'adif.NAME',
            'time'
        ]);
    });

    it('falls back to the defaults', () => {
        expect(normalizeColumns(undefined)).toEqual(DEFAULT_COLUMNS);
        expect(normalizeColumns({ call: true })).toEqual(DEFAULT_COLUMNS);
        expect(normalizeColumns(['nope'])).toEqual(DEFAULT_COLUMNS);
    });
});

describe('moveColumn', () => {
    it('moves a column left or right', () => {
        expect(moveColumn(['a', 'b', 'c'], 2, 0)).toEqual(['c', 'a', 'b']);
        expect(moveColumn(['a', 'b', 'c'], 0, 1)).toEqual(['b', 'a', 'c']);
    });

    it('ignores moves past either end', () => {
        expect(moveColumn(['a', 'b'], 0, -1)).toEqual(['a', 'b']);
        expect(moveColumn(['a', 'b'], 1, 2)).toEqual(['a', 'b']);
    });
});
