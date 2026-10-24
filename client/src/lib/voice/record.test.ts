import { describe, expect, it } from 'vitest';
import { defaultContest, type ContestSettings } from '#lib/qso/contest.ts';
import { newDraft, type Draft } from './draft.ts';
import { draftFields, draftProblem, queuedDuplicate } from './record.ts';

const at = new Date('2026-10-19T09:44:12Z');
const base = newDraft({ freq: '7.144', band: '40m', mode: 'SSB', submode: 'LSB' });
const draft: Draft = { ...base, call: 'JH1XYZ', rstRcvd: '57', startedAt: at.toISOString() };
const contest = (patch: Partial<ContestSettings> = {}): ContestSettings => ({
    ...defaultContest(),
    enabled: true,
    contestId: 'all-ja',
    ...patch
});

describe('draftProblem', () => {
    it('needs the call', () => {
        expect(draftProblem(base, defaultContest())).toBe('Say the call first.');
        expect(draftProblem(draft, defaultContest())).toBeUndefined();
    });

    it('refuses free text the server would refuse', () => {
        expect(draftProblem({ ...draft, qth: 'x'.repeat(101) }, defaultContest())).toBe(
            'QTH is longer than 100 characters.'
        );
    });

    it('needs the received number and usable settings in a contest', () => {
        expect(draftProblem(draft, contest())).toBe('Contest: say the received number.');
        expect(draftProblem({ ...draft, received: '25H' }, contest())).toBeUndefined();
        expect(
            draftProblem({ ...draft, received: '25H' }, contest({ exchangeTemplate: '{x}' }))
        ).toBe('Sent number: only {serial} and {serial:N} can be used.');
    });
});

describe('draftFields', () => {
    it('writes the QSO as ADIF fields at the time of logging', () => {
        expect(draftFields(draft, defaultContest(), at)).toEqual({
            CALL: 'JH1XYZ',
            QSO_DATE: '20261019',
            TIME_ON: '094412',
            FREQ: '7.144',
            BAND: '40m',
            MODE: 'SSB',
            SUBMODE: 'LSB',
            RST_SENT: '59',
            RST_RCVD: '57'
        });
    });

    it('leaves out a missing frequency', () => {
        expect(draftFields({ ...draft, freq: '' }, defaultContest(), at)).not.toHaveProperty(
            'FREQ'
        );
    });

    it('writes the QSL arrangement, the area code, name and QTH', () => {
        const spoken = {
            ...draft,
            qsl: 'requested' as const,
            area: '100110',
            name: 'Taro',
            qth: 'Minato'
        };
        const jcx = { name: 'CNTY', label: 'JCC / JCG' };
        expect(draftFields(spoken, defaultContest(), at, jcx)).toMatchObject({
            QSL_SENT: 'R',
            CNTY: '100110',
            NAME: 'Taro',
            QTH: 'Minato'
        });
        expect(draftFields(spoken, defaultContest(), at)).not.toHaveProperty('CNTY');
        const oneWay = draftFields({ ...draft, qsl: 'oneWay' }, defaultContest(), at);
        expect([oneWay.QSL_SENT, oneWay.QSL_RCVD]).toEqual(['N', 'R']);
        expect(draftFields(draft, defaultContest(), at)).not.toHaveProperty('QSL_SENT');
    });

    it('records both numbers in a contest', () => {
        const fields = draftFields({ ...draft, received: '1001M' }, contest({ nextSerial: 7 }), at);
        expect(fields).toMatchObject({
            CONTEST_ID: 'ALL-JA',
            STX_STRING: '007',
            SRX_STRING: '1001M'
        });
    });

    it('puts the RST in front when asked to', () => {
        const fields = draftFields(
            { ...draft, received: '25H' },
            contest({ includeRst: true, exchangeTemplate: '25M' }),
            at
        );
        expect([fields.STX_STRING, fields.SRX_STRING]).toEqual(['5925M', '5725H']);
    });

    it('keeps a number spoken outside a contest, but sends nothing', () => {
        const fields = draftFields({ ...draft, received: '005' }, defaultContest(), at);
        expect(fields.SRX_STRING).toBe('005');
        expect(fields).not.toHaveProperty('STX_STRING');
        expect(fields).not.toHaveProperty('CONTEST_ID');
    });
});

describe('queuedDuplicate', () => {
    const queued = [
        {
            id: 'a',
            fields: {
                CALL: 'jh1xyz',
                BAND: '40M',
                MODE: 'ssb',
                QSO_DATE: '20261019',
                TIME_ON: '090200'
            }
        },
        {
            id: 'b',
            fields: {
                CALL: 'JH1XYZ',
                BAND: '40m',
                MODE: 'SSB',
                QSO_DATE: '20261019',
                TIME_ON: '093500'
            }
        }
    ];

    it('finds the same call, band and mode within 15 minutes, ignoring case', () => {
        expect(queuedDuplicate(queued, draftFields(draft, defaultContest(), at))?.id).toBe('b');
    });

    it('ignores QSOs further apart or on another band', () => {
        expect(queuedDuplicate(queued.slice(0, 1), draftFields(draft, defaultContest(), at))).toBe(
            undefined
        );
        expect(
            queuedDuplicate(queued, draftFields({ ...draft, band: '20m' }, defaultContest(), at))
        ).toBeUndefined();
    });
});
