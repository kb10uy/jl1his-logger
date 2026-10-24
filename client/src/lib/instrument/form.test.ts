import { describe, expect, it } from 'vitest';
import type { Instrument } from '#lib/api/types.ts';
import { isPower } from '#lib/qso/power.ts';
import { draftOf, instrumentInput, instrumentProblem, sameInput } from './form.ts';

const instrument: Instrument = {
    id: 'i1',
    name: 'IC-705 QRP',
    my_rig: 'IC-705',
    my_antenna: null,
    tx_pwr: '5',
    fields: { MY_SIG: 'POTA' },
    archived_at: null,
    created_at: '2026-10-01T00:00:00Z',
    updated_at: '2026-10-01T00:00:00Z'
};

describe('instrument drafts', () => {
    it('round-trips an Instrument', () => {
        const draft = draftOf(instrument);
        expect(draft).toEqual({
            name: 'IC-705 QRP',
            rig: 'IC-705',
            antenna: '',
            power: '5',
            extra: [{ name: 'MY_SIG', value: 'POTA' }]
        });
        const input = instrumentInput(draft);
        expect(input).toEqual({
            name: 'IC-705 QRP',
            my_rig: 'IC-705',
            my_antenna: null,
            tx_pwr: '5',
            fields: { MY_SIG: 'POTA' }
        });
        expect(sameInput(input, { ...input, tx_pwr: '0.5' })).toBe(false);
    });

    it('drops empty values and upper-cases names', () => {
        const draft = draftOf();
        draft.name = ' FT-817 ';
        draft.power = ' 0.5 ';
        draft.extra = [
            { name: 'my_sig_info', value: 'JP-0012' },
            { name: 'MY_SIG', value: ' ' }
        ];
        expect(instrumentInput(draft)).toEqual({
            name: 'FT-817',
            my_rig: null,
            my_antenna: null,
            tx_pwr: '0.5',
            fields: { MY_SIG_INFO: 'JP-0012' }
        });
    });

    it.each([
        ['100', true],
        ['0.5', true],
        ['.25', true],
        ['5.', true],
        ['0.000001', true],
        ['0.0000001', false],
        ['0', false],
        ['QRP', false],
        ['-5', false],
        ['1e3', false]
    ])('reads %s as a TX power: %s', (value, valid) => {
        expect(isPower(value)).toBe(valid);
    });

    it.each([
        [{ name: '' }, 'An Instrument needs a name.'],
        [{ power: 'QRP' }, 'TX power is watts above zero, with at most six decimal places.'],
        [
            { extra: [{ name: 'my_rig', value: 'X' }] },
            'MY_RIG cannot be another field of an Instrument.'
        ],
        [{ extra: [{ name: 'MY RIG', value: 'X' }] }, 'MY RIG is not an ADIF QSO field.'],
        [
            {
                extra: [
                    { name: 'MY_SIG', value: 'X' },
                    { name: 'my_sig', value: 'Y' }
                ]
            },
            'MY_SIG is listed twice.'
        ]
    ])('refuses %o', (change, problem) => {
        expect(instrumentProblem({ ...draftOf(instrument), ...change })).toBe(problem);
    });

    it('accepts a complete draft', () => {
        expect(instrumentProblem(draftOf(instrument))).toBeUndefined();
    });
});
