import { describe, expect, it } from 'vitest';
import { stationMismatches } from './station.ts';

const station = {
    callsign: 'JL1HIS',
    fields: { MY_GRIDSQUARE: 'PM95uq', MY_CNTY: '100105' }
};

describe('stationMismatches', () => {
    it('finds nothing when the record agrees or leaves a value out', () => {
        expect(stationMismatches({ CALL: 'JA1ABC', STATION_CALLSIGN: 'jl1his' }, station)).toEqual(
            []
        );
    });

    it('takes a shorter grid square that begins the Station one as the same', () => {
        expect(stationMismatches({ MY_GRIDSQUARE: 'pm95' }, station)).toEqual([]);
        expect(stationMismatches({ MY_GRIDSQUARE: 'PM95UQ' }, station)).toEqual([]);
    });

    it('reports each value that differs from the Station', () => {
        expect(
            stationMismatches(
                { STATION_CALLSIGN: 'JL1HIS/P', MY_GRIDSQUARE: 'PM96', MY_CNTY: ' 100105 ' },
                station
            )
        ).toEqual([
            { name: 'MY_GRIDSQUARE', given: 'PM96', station: 'PM95uq' },
            { name: 'STATION_CALLSIGN', given: 'JL1HIS/P', station: 'JL1HIS' }
        ]);
    });
});
