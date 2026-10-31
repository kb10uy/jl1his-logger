import { describe, expect, it } from 'vitest';
import { bulkChange, fieldProblems, isEmpty, KEEP, NO_INSTRUMENT } from './bulk.ts';

const edit = (name: string, value = '', clear = false) => ({ name, value, clear });

describe('bulkChange', () => {
    it('sets, clears and skips fields and keeps the Station and Instrument by default', () => {
        const change = bulkChange(
            [
                edit('qsl_sent', 'Y'),
                edit(' name ', 'Taro'),
                edit('comment', '', true),
                edit('qth'),
                edit('', 'x')
            ],
            KEEP,
            KEEP
        );
        expect(change).toEqual({ fields: { QSL_SENT: 'Y', NAME: 'Taro', COMMENT: '' } });
        expect(isEmpty(change)).toBe(false);
    });

    it('names a Station and an Instrument or no Instrument', () => {
        expect(bulkChange([], 'st', 'in')).toEqual({
            fields: {},
            station_id: 'st',
            instrument_id: 'in'
        });
        expect(bulkChange([], KEEP, NO_INSTRUMENT)).toEqual({
            fields: {},
            instrument_id: null
        });
        expect(isEmpty(bulkChange([edit('NAME')], KEEP, KEEP))).toBe(true);
    });
});

describe('fieldProblems', () => {
    it('finds invalid and repeated names', () => {
        expect(
            fieldProblems([
                edit('QSL_SENT'),
                edit('qsl_sent'),
                edit('MY RIG'),
                edit('eor'),
                edit('name'),
                edit('NAME'),
                edit('')
            ])
        ).toEqual([undefined, 'repeated', 'name', 'name', undefined, 'repeated', undefined]);
    });
});
