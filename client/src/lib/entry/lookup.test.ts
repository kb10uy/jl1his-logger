import { describe, expect, it } from 'vitest';
import { candidateOf } from './lookup.ts';

describe('candidateOf', () => {
    const at = new Date('2026-10-22T04:15:30Z');

    it('asks for duplicates at the UTC time of entry', () => {
        expect(candidateOf('40m', 'CW', at)).toEqual({
            band: '40m',
            mode: 'CW',
            qso_date: '20261022',
            time_on: '041530'
        });
    });

    it('asks for none without a band or a mode', () => {
        expect(candidateOf('', 'CW', at)).toBeUndefined();
        expect(candidateOf('40m', '', at)).toBeUndefined();
    });
});
