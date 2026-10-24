import { describe, expect, it } from 'vitest';
import { withBand, withFreq, withMode } from './tuning.ts';

describe('tuning', () => {
    const ssb = {
        freq: '7.144',
        band: '40m',
        mode: 'SSB',
        submode: 'LSB',
        rstSent: '59',
        rstRcvd: '57'
    };

    it('moves the band and the sideband with a typed frequency', () => {
        expect(withFreq(ssb, '14.2')).toMatchObject({ freq: '14.2', band: '20m', submode: 'USB' });
        expect(withFreq(ssb, '7.2')).toMatchObject({ band: '40m', submode: 'LSB' });
        expect(withFreq(ssb, '')).toMatchObject({ freq: '', band: '40m' });
    });

    it('keeps a sideband chosen by hand while the band stays', () => {
        expect(withFreq({ ...ssb, submode: 'USB' }, '7.2').submode).toBe('USB');
    });

    it('sets the representative frequency of a chosen band', () => {
        expect(withBand(ssb, '20m')).toMatchObject({ freq: '14.000', band: '20m', submode: 'USB' });
        expect(withBand({ ...ssb, mode: 'CW', submode: '' }, '20m').submode).toBe('');
    });

    it('moves only reports still at the default when the mode changes', () => {
        expect(withMode(ssb, 'CW', '')).toMatchObject({ rstSent: '599', rstRcvd: '57' });
        expect(withMode({ ...ssb, mode: 'CW', rstSent: '599' }, 'FT8', '')).toMatchObject({
            rstSent: '-10',
            rstRcvd: '57'
        });
    });
});
