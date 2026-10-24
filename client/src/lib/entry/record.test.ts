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
    cnty: '',
    qsl: 'none',
    comment: '',
    txPwr: '',
    notes: '',
    extra: []
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

    it('records JCC / JCG and the QSL request', () => {
        expect(
            entryFields({ ...entry, cnty: ' 100101 ', qsl: 'requested' }, defaultContest(), at)
        ).toMatchObject({ CNTY: '100101', QSL_SENT: 'R' });
        const oneWay = entryFields({ ...entry, qsl: 'oneWay' }, defaultContest(), at);
        expect([oneWay.QSL_SENT, oneWay.QSL_RCVD]).toEqual(['N', 'R']);
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

    it('adds TX power, notes and other ADIF fields with a value', () => {
        const extra = {
            ...entry,
            txPwr: ' 10 ',
            notes: ' from the park ',
            extra: [
                { name: ' sota_ref ', value: ' JA/TK-001 ' },
                { name: 'app_test_x', value: 'y' },
                { name: 'GRIDSQUARE', value: ' ' }
            ]
        };
        expect(entryProblem(extra, defaultContest())).toBeUndefined();
        expect(entryFields(extra, defaultContest(), at)).toMatchObject({
            TX_PWR: '10',
            NOTES: 'from the park',
            SOTA_REF: 'JA/TK-001',
            APP_TEST_X: 'y'
        });
        expect(entryFields(extra, defaultContest(), at)).not.toHaveProperty('GRIDSQUARE');
    });

    it.each([
        [{ txPwr: '10W' }, 'TX power is watts above zero, with at most six decimal places.'],
        [{ txPwr: '0' }, 'TX power is watts above zero, with at most six decimal places.'],
        [{ extra: [{ name: 'SOTA', value: 'x' }] }, 'SOTA is not an ADIF QSO field.'],
        [{ extra: [{ name: 'call', value: 'x' }] }, 'CALL has its own field.'],
        [{ extra: [{ name: 'VE_PROV', value: 'x' }] }, 'VE_PROV is not an ADIF QSO field.'],
        [
            { extra: [{ name: 'STATION_CALLSIGN', value: 'x' }] },
            'STATION_CALLSIGN comes from the Station.'
        ],
        [{ extra: [{ name: '', value: 'x' }] }, 'Name the extra field, or clear its value.'],
        [
            {
                extra: [
                    { name: 'age', value: '1' },
                    { name: 'AGE', value: '2' }
                ]
            },
            'AGE is entered twice.'
        ]
    ])('refuses extra fields such as %j', (change, problem) => {
        expect(entryProblem({ ...entry, ...change }, defaultContest())).toBe(problem);
    });

    it('says what blocks logging', () => {
        expect(entryProblem({ ...entry, call: '' }, defaultContest())).toBe('Enter the call.');
        expect(entryProblem(entry, { ...defaultContest(), enabled: true })).toMatch(/received/);
        expect(entryProblem(entry, defaultContest())).toBeUndefined();
    });
});
