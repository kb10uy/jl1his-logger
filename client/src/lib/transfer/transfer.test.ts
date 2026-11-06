import { describe, expect, it } from 'vitest';
import type { Batch, BatchPage, MergeReport, NewReport, Qso } from '#lib/api/types.ts';
import {
    type ExportLists,
    SPLIT_FIELDS,
    appendPage,
    batchState,
    defaultSplit,
    exportCount,
    exportRuns,
    differencesText,
    fileName,
    formatOf,
    missingCount,
    outcomeText,
    placeExport,
    readingNote,
    recordOnly,
    rejectCount,
    reviseExport,
    splitChoices,
    splitLabel
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
        expect(fileName('JL1HIS', 'lotw', new Date('2026-10-23T09:05:00Z'), '', 'zip')).toBe(
            'JL1HIS-lotw-20261023-0905.zip'
        );
    });

    it('offers the split choices of a kind with its default among them', () => {
        expect(defaultSplit('lotw')).toBe('callsign_location');
        expect(defaultSplit('eqsl')).toBe('callsign');
        expect(defaultSplit('pota')).toBe('callsign_park');
        expect(defaultSplit('qsl_card')).toBe('none');
        expect(defaultSplit('adif')).toBe('none');
        for (const kind of ['adif', 'pota', 'eqsl', 'lotw', 'qsl_card'] as const) {
            expect(splitChoices(kind)).toContain(defaultSplit(kind));
        }
        expect(splitChoices('pota')).not.toContain('callsign_location');
        expect(SPLIT_FIELDS.callsign_park).toEqual(['STATION_CALLSIGN', 'MY_SIG_INFO']);
        expect(splitLabel('callsign_location')).toBe(
            'STATION_CALLSIGN, MY_STATE, MY_CNTY, MY_GRIDSQUARE'
        );
        expect(splitLabel('none')).toBe('No (one file)');
    });

    it('counts the records of files that lack a split field', () => {
        const batch = {} as Batch;
        expect(
            missingCount([
                { name: 'JL1HIS_PM95.adi', label: 'JL1HIS PM95', missing: [], records: 5, batch },
                {
                    name: 'JL1HIS_none.adi',
                    label: 'JL1HIS none',
                    missing: ['MY_GRIDSQUARE'],
                    records: 2,
                    batch
                },
                {
                    name: 'none_none.adi',
                    label: 'none none',
                    missing: ['STATION_CALLSIGN', 'MY_GRIDSQUARE'],
                    records: 1,
                    batch
                }
            ])
        ).toBe(3);
        expect(missingCount([])).toBe(0);
    });

    it('groups batches into exports and counts their states', () => {
        const batch = (over: Partial<Batch>): Batch => ({
            id: '1',
            export_id: 'a',
            kind: 'lotw',
            created_by: 'u',
            created_at: '2026-11-05T10:00:01Z',
            applied_at: null,
            dismissed_at: null,
            qso_count: 1,
            label: null,
            ...over
        });
        const runs = exportRuns([
            batch({ id: '4', export_id: 'b', kind: 'adif', qso_count: 9 }),
            batch({ id: '3', label: 'JL1HIS/P', dismissed_at: '2026-11-05T11:00:00Z' }),
            batch({ id: '2', label: 'JL1HIS', applied_at: '2026-11-05T11:00:00Z', qso_count: 2 }),
            batch({ id: '1', label: 'JA1ZZZ', created_at: '2026-11-05T10:00:00Z' })
        ]);
        expect(runs.map((r) => [r.id, r.kind, r.qsos, r.pending, r.applied, r.dismissed])).toEqual([
            ['b', 'adif', 9, 0, 0, 0],
            ['a', 'lotw', 4, 1, 1, 1]
        ]);
        expect(runs[1].created_at).toBe('2026-11-05T10:00:00Z');
        expect(runs[1].batches.map((b) => b.id)).toEqual(['1', '2', '3']);
        expect(batchState(runs[0].batches[0])).toBe('record');
        expect(runs[1].batches.map(batchState)).toEqual(['pending', 'applied', 'dismissed']);
    });

    describe('export lists', () => {
        const batch = (id: string, over: Partial<Batch> = {}): Batch => ({
            id,
            export_id: id,
            kind: 'lotw',
            created_by: 'u',
            created_at: '2026-11-05T10:00:00Z',
            applied_at: null,
            dismissed_at: null,
            qso_count: 1,
            label: null,
            ...over
        });
        const dismissed = (b: Batch) => ({ ...b, dismissed_at: '2026-11-05T11:00:00Z' });
        const ids = (list: BatchPage) => list.batches.map((b) => b.id);
        const lists: ExportLists = {
            pending: {
                batches: [batch('p9'), batch('p7'), batch('p5', { export_id: 'p6' })],
                next: 'p6',
                total: 8
            },
            finished: {
                batches: [batch('r8', { kind: 'adif' }), batch('a4', { kind: 'pota' })],
                next: null,
                total: 2
            }
        };

        it('counts exports, not batches', () => {
            expect(exportCount([batch('1'), batch('2', { export_id: '1' }), batch('3')])).toBe(2);
        });

        it('puts a new export at the top of its list', () => {
            const pending = placeExport(lists, [
                batch('pa', { export_id: 'pz' }),
                batch('pb', { export_id: 'pz' })
            ]);
            expect(ids(pending.pending).slice(0, 3)).toEqual(['pa', 'pb', 'p9']);
            expect(pending.pending.total).toBe(9);
            expect(pending.finished).toBe(lists.finished);

            const record = placeExport(lists, [batch('zz', { kind: 'adif' })]);
            expect(ids(record.finished)).toEqual(['zz', 'r8', 'a4']);
            expect(record.finished.total).toBe(3);
        });

        it('keeps an export in its place while it has something pending', () => {
            const split = placeExport(lists, [
                batch('q2', { export_id: 'p8' }),
                batch('q1', { export_id: 'p8' })
            ]);
            const marked = reviseExport(split, 'p8', (b) => (b.id === 'q1' ? dismissed(b) : b));
            expect(ids(marked.pending)).toEqual(['p9', 'q2', 'q1', 'p7', 'p5']);
            expect(marked.pending.total).toBe(9);
        });

        it('moves an export between the lists when its state changes', () => {
            const done = reviseExport(lists, 'p7', dismissed);
            expect(ids(done.pending)).toEqual(['p9', 'p5']);
            expect(done.pending.total).toBe(7);
            expect(ids(done.finished)).toEqual(['r8', 'p7', 'a4']);
            expect(done.finished.total).toBe(3);

            const back = reviseExport(done, 'p7', (b) => ({ ...b, dismissed_at: null }));
            expect(back).toEqual(lists);
        });

        it('leaves an export older than the loaded rows to its page', () => {
            const older = placeExport(lists, [batch('p1')]);
            expect(ids(older.pending)).toEqual(ids(lists.pending));
            expect(older.pending.total).toBe(9);
            expect(older.pending.next).toBe('p6');
        });

        it('takes the next page and its count from the server', () => {
            const page = { batches: [batch('p4')], next: null, total: 7 };
            expect(appendPage(lists.pending, page)).toEqual({
                batches: [...lists.pending.batches, batch('p4')],
                next: null,
                total: 7
            });
        });

        it('ignores an export it has not loaded', () => {
            expect(reviseExport(lists, 'nowhere', dismissed)).toBe(lists);
        });
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

    it('notes how a file was read', () => {
        expect(readingNote({ encoding: 'utf-8', lengths: [] })).toBe('Read as UTF-8');
        expect(readingNote({ encoding: 'shift_jis', lengths: ['bytes', 'utf16'] })).toBe(
            'Read as Shift_JIS · non-ASCII lengths in Bytes, UTF-16 units'
        );
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
