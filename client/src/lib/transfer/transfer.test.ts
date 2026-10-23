import { describe, expect, it } from 'vitest';
import type { MergeReport, NewReport, Qso } from '#lib/api/types.ts';
import {
    differencesText,
    fileName,
    formatOf,
    outcomeText,
    recordOnly,
    rejectCount
} from './transfer.ts';

describe('transfer helpers', () => {
    it('reads the format from the file name', () => {
        expect(formatOf('lotwreport.ADX')).toBe('adx');
        expect(formatOf('wavelog.adi')).toBe('adi');
        expect(formatOf('export')).toBe('adi');
    });

    it('names exported files', () => {
        expect(fileName('JL1HIS / club', 'qsl_card', new Date('2026-10-23T09:05:00Z'))).toBe(
            'JL1HIS_club-qsl-card-20261023-0905.adi'
        );
        expect(fileName('JL1HIS', 'adif', new Date('2026-10-23T09:05:00Z'), '-rejects')).toBe(
            'JL1HIS-adif-rejects-20261023-0905.adi'
        );
    });

    it('tells records from batches that can be applied', () => {
        expect(recordOnly('pota')).toBe(true);
        expect(recordOnly('adif')).toBe(true);
        expect(recordOnly('lotw')).toBe(false);
    });

    it('counts rejects', () => {
        const merge = {
            invalid: [{ record: 1, error: 'x' }],
            unmatched: [2, 3],
            ambiguous: [{ record: 4, ids: ['a', 'b'] }]
        } as MergeReport;
        expect(rejectCount(merge)).toBe(4);
        expect(rejectCount({ invalid: [{ record: 1, error: 'x' }] } as NewReport)).toBe(1);
    });

    it('lists differences, largest first', () => {
        expect(differencesText({ CNTY: 3, GRIDSQUARE: 14, IOTA: 0 })).toBe('GRIDSQUARE 14, CNTY 3');
    });

    it('describes an outcome', () => {
        const qso = {} as Qso;
        expect(outcomeText({ updated: [qso, qso], skipped: [] })).toBe('2 updated');
        expect(outcomeText({ updated: [qso], skipped: [{ id: 'a', reason: 'deleted' }] })).toBe(
            '1 updated, 1 skipped'
        );
    });
});
