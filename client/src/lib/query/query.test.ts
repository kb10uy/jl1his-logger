import { describe, expect, it } from 'vitest';
import { analyze, appendTerm, matcher, removeToken } from './index.ts';
import { chip } from './chips.ts';

describe('analyze', () => {
    it('keeps every term with its span, even bad ones', () => {
        const tokens = analyze('名前 band:40m,20m bogus:1');
        expect(tokens.map((t) => [t.start, t.end, t.key, t.values, t.error?.code ?? null])).toEqual(
            [
                [0, 2, null, ['名前'], null],
                [3, 15, 'band', ['40m', '20m'], null],
                [16, 23, 'bogus', ['1'], 'unknown_field']
            ]
        );
    });
});

describe('removeToken', () => {
    it('removes a term by its character span', () => {
        const query = '名前 -mode:FT8  band:40m';
        const [, mode] = analyze(query);
        expect(removeToken(query, mode)).toBe('名前 band:40m');
    });

    it('appends a term with one space', () => {
        expect(appendTerm(' band:40m ', 'call:')).toBe('band:40m call:');
        expect(appendTerm('', 'call:')).toBe('call:');
    });
});

describe('matcher', () => {
    const fields = { CALL: 'JA1XYZ/P', BAND: '40m', MODE: 'SSB', QSO_DATE: '20261019' };

    it('matches rows as the server does', () => {
        expect(matcher('call:JA1* -mode:FT8 date:2026-10-19')?.matches(fields, 'Home')).toBe(true);
        expect(matcher('station:takao')?.matches(fields, 'Mt. Takao')).toBe(true);
        expect(matcher('band:20m')?.matches(fields, undefined)).toBe(false);
    });

    it('has nothing for a bad query', () => {
        expect(matcher('qrg:7')).toBeUndefined();
        expect(matcher('freq:7MHz')).toBeUndefined();
    });

    it('compares frequencies and powers as numbers', () => {
        const row = { ...fields, FREQ: '7.0410', TX_PWR: '5.0', LOTW_QSL_SENT: 'y' };
        expect(matcher('freq:7.0..7.2 pwr:<=5 lotw-sent:Y')?.matches(row, undefined)).toBe(true);
        expect(matcher('freq:>7.041')?.matches(row, undefined)).toBe(false);
        expect(matcher('pwr:>5')?.matches(row, undefined)).toBe(false);
    });
});

describe('chip', () => {
    const chips = (query: string) => analyze(query).map(chip);

    it('describes terms in words', () => {
        expect(chips('call:JH1* -band:40m,20m sent:R date:2026-10-01.. no:freq')).toEqual([
            { label: 'Call', op: 'matches', value: 'JH1*', tone: 'normal' },
            { label: 'Band', op: 'is none of', value: '40m, 20m', tone: 'negated' },
            { label: 'QSL sent', op: 'is', value: 'R', tone: 'normal' },
            { label: 'Date', op: 'from', value: '2026-10-01', tone: 'normal' },
            { label: 'FREQ', op: 'is missing', value: '', tone: 'normal' }
        ]);
    });

    it('marks unfinished and bad terms', () => {
        expect(chips('call: qrg:7 freq:7MHz -yokohama')).toEqual([
            { label: 'Call', op: 'is', value: '…', tone: 'pending' },
            { label: 'qrg:7', op: 'unknown field', value: '', tone: 'error' },
            { label: 'freq:7MHz', op: 'bad number', value: '', tone: 'error' },
            { label: 'Text', op: 'excludes', value: '"yokohama"', tone: 'negated' }
        ]);
    });

    it('reads date comparisons', () => {
        expect(
            chips('date:>2026-10-18 -date:2026-10-19 date:..2026-10-20').map((c) => c.op)
        ).toEqual(['after', 'not on', 'until']);
    });

    it('reads frequency, power and LoTW / eQSL terms', () => {
        expect(chips('freq:14.0..14.35 pwr:<=5 -pwr:>100 eqsl-rcvd:Y,V no:lotw-sent')).toEqual([
            { label: 'Freq', op: 'between', value: '14.0 – 14.35 MHz', tone: 'normal' },
            { label: 'TX power', op: 'at most', value: '5 W', tone: 'normal' },
            { label: 'TX power', op: 'not above', value: '100 W', tone: 'negated' },
            { label: 'eQSL rcvd', op: 'is any of', value: 'Y, V', tone: 'normal' },
            { label: 'LoTW sent', op: 'is missing', value: '', tone: 'normal' }
        ]);
    });
});
