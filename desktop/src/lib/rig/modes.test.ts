import { describe, expect, it } from 'vitest';
import { MODE_CHOICES, modeKey } from '#client/qso/modes.ts';
import { RIG_MODES, fitting, followRig, loggedFor, pickMode, rigModeOf } from './modes.ts';

describe('rigModeOf', () => {
    it.each([
        ['USB', 'USB'],
        [' pktusb ', 'PKTUSB'],
        ['CWR', 'CW'],
        ['RTTYR', 'RTTY'],
        ['WFM', 'FM'],
        ['FAX', undefined]
    ])('reads %s as %s', (reported, expected) => {
        expect(rigModeOf(reported)).toBe(expected);
    });
});

describe('fitting', () => {
    it('only names modes the mode picker offers', () => {
        const known = new Set(MODE_CHOICES.map((c) => modeKey(c.mode, c.submode)));
        for (const rig of RIG_MODES) {
            expect(fitting(rig).filter((k) => !known.has(k))).toEqual([]);
            expect(fitting(rig)).toContain(loggedFor(rig, {}));
        }
    });

    it('lets data modes ride on a voice sideband', () => {
        expect(fitting('USB')).toEqual(expect.arrayContaining(['SSB|USB', 'FT8|', 'MFSK|FT4']));
        expect(fitting('PKTUSB')).not.toContain('SSB|USB');
    });
});

describe('followRig', () => {
    it('keeps a logged mode the rig mode fits', () => {
        expect(followRig('PKTUSB', 'MFSK|FT4', {})).toEqual({ key: 'MFSK|FT4', fit: 'kept' });
        expect(followRig('USB', 'FT8|', {})).toEqual({ key: 'FT8|', fit: 'kept' });
    });

    it('moves the SSB sideband with the rig', () => {
        expect(followRig('LSB', 'SSB|USB', {})).toEqual({ key: 'SSB|LSB', fit: 'kept' });
    });

    it('follows to the remembered choice, else the default', () => {
        expect(followRig('PKTUSB', 'SSB|USB', {})).toEqual({ key: 'FT8|', fit: 'followed' });
        expect(followRig('PKTUSB', 'SSB|USB', { PKTUSB: 'MFSK|FT4' })).toEqual({
            key: 'MFSK|FT4',
            fit: 'followed'
        });
        expect(followRig('FM', 'CW|', { FM: 'SSB|USB' })).toEqual({ key: 'FM|', fit: 'followed' });
    });

    it('keeps anything when the rig mode is unknown', () => {
        expect(followRig(undefined, 'CW|', {})).toEqual({ key: 'CW|', fit: 'kept' });
    });
});

describe('pickMode', () => {
    it('remembers a choice among several fitting modes', () => {
        expect(pickMode('PKTUSB', 'MFSK|FT4', {})).toEqual({
            fit: 'kept',
            prefs: { PKTUSB: 'MFSK|FT4' }
        });
    });

    it('keeps the remembered choices for a single or the same choice', () => {
        const prefs = { FM: 'DIGITALVOICE|C4FM' };
        expect(pickMode('AM', 'AM|', prefs).prefs).toBe(prefs);
        expect(pickMode('FM', 'DIGITALVOICE|C4FM', prefs).prefs).toBe(prefs);
    });

    it('marks a mode the rig mode does not fit', () => {
        expect(pickMode('CW', 'SSB|USB', {})).toEqual({ fit: 'mismatch', prefs: {} });
    });
});
