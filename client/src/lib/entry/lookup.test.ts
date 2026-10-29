import { describe, expect, it } from 'vitest';
import type { Lookup } from '#lib/api/types.ts';
import { candidateOf, lookupLine } from './lookup.ts';

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

describe('lookupLine', () => {
    it('joins the entity and what earlier QSOs recorded', () => {
        const lookup: Lookup = {
            call: 'JH1XYZ',
            entity: {
                name: 'Japan',
                dxcc: 339,
                continent: 'AS',
                cq_zone: 25,
                itu_zone: 45,
                latitude: 0,
                longitude: 0
            },
            previous: { count: 3, last: null, fields: { NAME: 'Taro', QTH: 'Tokyo' } }
        };
        expect(lookupLine(lookup)).toBe('Japan · DXCC 339 · CQ 25 · ITU 45 · Taro · Tokyo');
    });

    it('is empty for an unknown call never worked', () => {
        expect(
            lookupLine({
                call: 'X',
                entity: null,
                previous: { count: 0, last: null, fields: {} }
            })
        ).toBe('');
    });
});
