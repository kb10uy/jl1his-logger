import { describe, expect, it } from 'vitest';
import { draftOf, fieldsOf, missing, problem } from './edit.ts';

describe('QSO drafts', () => {
    const original = {
        CALL: 'ja1xyz',
        QSO_DATE: '20261019',
        TIME_ON: '0931',
        BAND: '40m',
        MODE: 'SSB',
        SUBMODE: 'LSB',
        NAME: 'Yuki',
        QSL_SENT: 'r',
        QSLRDATE: '20261020',
        TX_PWR: '5',
        MY_POTA_REF: 'JP-0012',
        APP_WAVELOG_ID: '48213',
        GRIDSQUARE: 'PM95 '
    };

    it('reads fields into form values', () => {
        expect(draftOf(original)).toMatchObject({
            call: 'ja1xyz',
            date: '2026-10-19',
            time: '09:31',
            qslSent: 'R',
            qslRcvdDate: '2026-10-20',
            txPwr: '5',
            freq: '',
            extra: [
                { name: 'APP_WAVELOG_ID', value: '48213' },
                { name: 'GRIDSQUARE', value: 'PM95 ' }
            ]
        });
    });

    it('writes the form back, keeping the other fields and dropping cleared ones', () => {
        const draft = { ...draftOf(original), name: '', freq: '7.144', time: '09:31:05' };
        expect(fieldsOf(draft, original)).toEqual({
            CALL: 'ja1xyz',
            QSO_DATE: '20261019',
            TIME_ON: '093105',
            FREQ: '7.144',
            BAND: '40m',
            MODE: 'SSB',
            SUBMODE: 'LSB',
            QSL_SENT: 'r',
            QSLRDATE: '20261020',
            TX_PWR: '5',
            MY_POTA_REF: 'JP-0012',
            APP_WAVELOG_ID: '48213',
            GRIDSQUARE: 'PM95 '
        });
    });

    it('writes a changed call in capitals', () => {
        expect(fieldsOf({ ...draftOf(original), call: 'ja1abc' }, original).CALL).toBe('JA1ABC');
        expect(fieldsOf(draftOf(original), original)).toEqual(original);
    });

    it('edits, adds and removes the other fields', () => {
        const draft = draftOf(original);
        draft.extra = [
            { name: 'gridsquare', value: ' PM95 ' },
            { name: 'iota', value: 'AS-007' },
            { name: 'SOTA_REF', value: '' }
        ];
        const fields = fieldsOf(draft, original);
        expect(fields.GRIDSQUARE).toBe('PM95 ');
        expect(fields.IOTA).toBe('AS-007');
        expect(fields).not.toHaveProperty('APP_WAVELOG_ID');
        expect(fields).not.toHaveProperty('SOTA_REF');
    });

    it('names the missing required fields', () => {
        expect(missing({ ...draftOf(original), call: ' ', mode: '' })).toEqual(['Call', 'Mode']);
        expect(problem({ ...draftOf(original), call: ' ' })).toBe('Needed before saving: Call.');
    });

    it('refuses a bad power and misplaced extra fields', () => {
        const draft = draftOf(original);
        expect(problem(draft)).toBeUndefined();
        expect(problem({ ...draft, txPwr: '0' })).toMatch(/TX power/);
        expect(problem({ ...draft, txPwr: '5.' })).toBeUndefined();
        const extra = (name: string) => problem({ ...draft, extra: [{ name, value: 'x' }] });
        expect(extra('')).toBe('Name the extra field, or clear its value.');
        expect(extra('tx_pwr')).toBe('TX_PWR has its own field.');
        expect(extra('MY_CITY')).toBe('MY_CITY comes from the Station.');
        expect(extra('FOO')).toBe('FOO is not an ADIF QSO field.');
        expect(extra('APP_X')).toBeUndefined();
        expect(
            problem({
                ...draft,
                extra: [
                    { name: 'IOTA', value: 'a' },
                    { name: 'iota', value: 'b' }
                ]
            })
        ).toBe('IOTA is entered twice.');
    });
});
