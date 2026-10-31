import { describe, expect, it } from 'vitest';
import { fieldChoices } from './enumerations.ts';

const values = (name: string) => fieldChoices(name)?.map(([value]) => value);

describe('fieldChoices', () => {
    it('offers the values of the enumerations the client knows', () => {
        expect(values('qsl_sent')).toEqual(['N', 'R', 'Q', 'Y', 'I']);
        expect(values('LOTW_QSL_RCVD')).toEqual(['N', 'R', 'Y', 'V', 'I']);
        expect(values('QSL_RCVD_VIA')).toEqual(['B', 'D', 'E', 'M']);
        expect(values('BAND_RX')).toContain('40m');
        expect(values('MODE')).toContain('FT8');
        expect(fieldChoices('DXCC')).toContainEqual(['339', 'JAPAN']);
    });

    it('leaves other fields to free input', () => {
        for (const name of ['NAME', 'QSLSDATE', 'SUBMODE', 'STATE', 'CNTY', 'PROP_MODE', 'APP_X']) {
            expect(fieldChoices(name)).toBeUndefined();
        }
    });
});
