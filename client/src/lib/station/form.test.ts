import { describe, expect, it } from 'vitest';
import type { Station } from '#lib/api/types.ts';
import { draftOf, sameInput, splitParks, stationInput, stationProblem } from './form.ts';

const station: Station = {
    id: 's1',
    wavelog_id: 3,
    name: 'Mt. Takao',
    callsign: 'JL1HIS/1',
    fields: {
        MY_GRIDSQUARE: 'PM95sp',
        MY_POTA_REF: 'JP-0012,JP-0008',
        MY_SIG: 'POTA'
    },
    archived_at: null,
    created_at: '2026-10-01T00:00:00Z',
    updated_at: '2026-10-01T00:00:00Z'
};

describe('station drafts', () => {
    it('round-trips a Station', () => {
        const draft = draftOf(station);
        expect(draft.parks).toEqual(['JP-0012', 'JP-0008']);
        expect(draft.extra).toEqual([{ name: 'MY_SIG', value: 'POTA' }]);
        expect(draft.main.MY_CITY).toBe('');
        const input = stationInput(draft);
        expect(input).toEqual({
            name: 'Mt. Takao',
            callsign: 'JL1HIS/1',
            wavelog_id: 3,
            fields: station.fields
        });
        expect(sameInput(input, { ...input, fields: { MY_SIG: 'POTA', ...input.fields } })).toBe(
            true
        );
        expect(sameInput(input, { ...input, wavelog_id: null })).toBe(false);
    });

    it('drops empty values and upper-cases names', () => {
        const draft = draftOf();
        draft.name = ' Home ';
        draft.callsign = 'jl1his';
        draft.main.MY_CITY = '  ';
        draft.extra = [
            { name: 'my_sig_info', value: 'JP-0012' },
            { name: 'MY_SIG', value: '' }
        ];
        expect(stationInput(draft)).toEqual({
            name: 'Home',
            callsign: 'JL1HIS',
            wavelog_id: null,
            fields: { MY_SIG_INFO: 'JP-0012' }
        });
    });

    it('splits parks', () => {
        expect(splitParks('jp-0012, JP-0008 jp-0012')).toEqual(['JP-0012', 'JP-0008']);
        expect(splitParks(undefined)).toEqual([]);
    });

    it.each([
        [{ name: '' }, 'A Station needs a name.'],
        [{ wavelogId: '0' }, 'The Wavelog ID is a positive whole number.'],
        [
            { extra: [{ name: 'STATION_CALLSIGN', value: 'X' }] },
            'STATION_CALLSIGN cannot be a Station default.'
        ],
        [{ extra: [{ name: 'MY RIG', value: 'X' }] }, 'MY RIG is not an ADIF field name.'],
        [{ extra: [{ name: 'my_rig', value: 'X' }] }, 'MY_RIG belongs to an Instrument.'],
        [{ extra: [{ name: 'tx_pwr', value: '5' }] }, 'TX_PWR belongs to an Instrument.'],
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
        expect(stationProblem({ ...draftOf(station), ...change })).toBe(problem);
    });

    it('accepts a complete draft', () => {
        expect(stationProblem(draftOf(station))).toBeUndefined();
    });
});
