import { representativeFreq } from '#lib/qso/bands.ts';
import { defaultContest, type ContestSettings } from '#lib/qso/contest.ts';

export interface EntrySettings {
    stationId: string;
    instrumentId: string;
    freq: string;
    band: string;
    mode: string;
    submode: string;
    contest: ContestSettings;
}

export function defaultEntry(): EntrySettings {
    return {
        stationId: '',
        instrumentId: '',
        freq: representativeFreq('40m') ?? '',
        band: '40m',
        mode: 'SSB',
        submode: 'LSB',
        contest: defaultContest()
    };
}
