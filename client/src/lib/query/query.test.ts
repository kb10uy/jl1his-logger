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
        expect(matcher('freq:7')).toBeUndefined();
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
        expect(chips('call: freq:7 -yokohama')).toEqual([
            { label: 'Call', op: 'is', value: '…', tone: 'pending' },
            { label: 'freq:7', op: 'unknown field', value: '', tone: 'error' },
            { label: 'Text', op: 'excludes', value: '"yokohama"', tone: 'negated' }
        ]);
    });

    it('reads date comparisons', () => {
        expect(
            chips('date:>2026-10-18 -date:2026-10-19 date:..2026-10-20').map((c) => c.op)
        ).toEqual(['after', 'not on', 'until']);
    });
});
