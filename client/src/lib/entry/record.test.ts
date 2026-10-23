import { describe, expect, it } from 'vitest';
import { defaultContest } from '#lib/qso/contest.ts';
import { entryFields, entryProblem, type Entry } from './record.ts';

const entry: Entry = {
    call: ' jh1xyz ',
    freq: '',
    band: '40m',
    mode: 'SSB',
    submode: 'LSB',
    rstSent: '59',
    rstRcvd: '57',
    received: '',
    name: '',
    qth: 'Tokyo',
    comment: ''
};
const at = new Date('2026-10-19T09:44:12Z');

describe('entry records', () => {
    it('writes the ADIF fields, leaving out empty ones and FREQ when missing', () => {
        expect(entryFields(entry, defaultContest(), at)).toEqual({
            CALL: 'JH1XYZ',
            QSO_DATE: '20261019',
            TIME_ON: '094412',
            BAND: '40m',
            MODE: 'SSB',
            SUBMODE: 'LSB',
            RST_SENT: '59',
            RST_RCVD: '57',
            QTH: 'Tokyo'
        });
    });

    it('records both exchanges in a contest', () => {
        const contest = {
            enabled: true,
            contestId: 'all-ja1',
            exchangeTemplate: '{serial}M',
            nextSerial: 7,
            includeRst: true
        };
        expect(entryFields({ ...entry, received: '1001h' }, contest, at)).toMatchObject({
            CONTEST_ID: 'ALL-JA1',
            STX_STRING: '59007M',
            SRX_STRING: '571001H'
        });
    });

    it('says what blocks logging', () => {
        expect(entryProblem({ ...entry, call: '' }, defaultContest())).toBe('Enter the call.');
        expect(entryProblem(entry, { ...defaultContest(), enabled: true })).toMatch(/received/);
        expect(entryProblem(entry, defaultContest())).toBeUndefined();
    });
});
