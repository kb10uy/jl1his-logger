import { describe, expect, it } from 'vitest';
import { draftOf, fieldsOf, missing } from './edit.ts';

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
        MY_POTA_REF: 'JP-0012',
        APP_WAVELOG_ID: '48213'
    };

    it('reads fields into form values', () => {
        expect(draftOf(original)).toMatchObject({
            call: 'ja1xyz',
            date: '2026-10-19',
            time: '09:31',
            qslSent: 'R',
            freq: ''
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
            MY_POTA_REF: 'JP-0012',
            APP_WAVELOG_ID: '48213'
        });
    });

    it('writes a changed call in capitals', () => {
        expect(fieldsOf({ ...draftOf(original), call: 'ja1abc' }, original).CALL).toBe('JA1ABC');
        expect(fieldsOf(draftOf(original), original)).toEqual(original);
    });

    it('names the missing required fields', () => {
        expect(missing({ ...draftOf(original), call: ' ', mode: '' })).toEqual(['Call', 'Mode']);
    });
});
