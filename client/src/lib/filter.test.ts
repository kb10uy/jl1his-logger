import { describe, expect, it } from 'vitest';
import { matchesFilter } from './filter.ts';

const values = ['Yakushima', 'JR6/JL1HIS', 'PM50', null, 'JP-0046'];

describe('matchesFilter', () => {
    it.each([
        ['', true],
        ['   ', true],
        ['yaku', true],
        ['jr6/', true],
        ['jp-0046', true],
        ['pm50 yaku', true],
        ['pm50 hokkaido', false],
        ['null', false]
    ])('matches %j: %s', (query, expected) => {
        expect(matchesFilter(values, query)).toBe(expected);
    });

    it('matches Japanese text as typed', () => {
        expect(matchesFilter(['宗谷岬 — 日本最北端の地'], '最北端')).toBe(true);
    });
});
