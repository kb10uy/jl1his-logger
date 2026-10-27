import { describe, expect, it } from 'vitest';
import type { Entry } from '#client/entry/record.ts';
import { applyFields } from './form.ts';

const entry: Entry = {
    call: '',
    freq: '7.130',
    band: '40m',
    mode: 'SSB',
    submode: 'LSB',
    rstSent: '59',
    rstRcvd: '59',
    received: '',
    name: '',
    qth: 'Tokyo',
    area: '',
    qsl: 'none',
    comment: '',
    txPwr: '',
    notes: '',
    extra: [{ name: 'MY_SIG', value: 'WWFF' }]
};
const area = { name: 'CNTY', label: 'JCC / JCG' };

describe('applyFields', () => {
    it('fills the fields of the form it names and leaves the rest', () => {
        expect(
            applyFields(
                entry,
                { call: 'ja1abc', RST_RCVD: '579', SRX_STRING: '13M', CNTY: '100105' },
                area
            )
        ).toEqual({
            ...entry,
            call: 'ja1abc',
            rstRcvd: '579',
            received: '13M',
            area: '100105'
        });
    });

    it('clears a field given an empty value', () => {
        expect(applyFields(entry, { QTH: '', MY_SIG: '' }, area)).toEqual({
            ...entry,
            qth: '',
            extra: []
        });
    });

    it('tunes the form like the band, frequency and mode controls', () => {
        const tuned = applyFields(entry, { FREQ: '14.074', MODE: 'FT8', RST_SENT: '-10' }, area);
        expect(tuned).toMatchObject({
            freq: '14.074',
            band: '20m',
            mode: 'FT8',
            submode: '',
            rstSent: '-10',
            rstRcvd: '-10'
        });
    });

    it('takes other QSO fields as extra fields and ignores what the form cannot hold', () => {
        const filled = applyFields(
            entry,
            { MY_SIG: 'POTA', SIG_INFO: 'JA-0001', TIME_ON: '1200', STATION_CALLSIGN: 'JL1HIS' },
            null
        );
        expect(filled.extra).toEqual([
            { name: 'MY_SIG', value: 'POTA' },
            { name: 'SIG_INFO', value: 'JA-0001' }
        ]);
        expect(filled.call).toBe('');
    });
});
