import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';
import { BANDS, bandOf, frequencyHz, representativeFreq } from './bands.ts';
import { MODE_CHOICES, followsBand, modeKey, parseModeKey, sidebandFor } from './modes.ts';
import { uuidv7 } from './uuid.ts';
import { compareNewestFirst, hhmm, isoDate, parks, rst, utcFields } from './format.ts';
import { qslLabel, qslTone } from './qsl.ts';
import {
    contestProblem,
    defaultContest,
    exchangeString,
    expandExchange,
    usesSerial
} from './contest.ts';

const fixture = <T>(path: string): T =>
    JSON.parse(
        readFileSync(new URL(`../../../../fixtures/reference/${path}`, import.meta.url), 'utf8')
    );

describe('bands', () => {
    it('match the server table', () => {
        const { bands } = fixture<{
            bands: { band: string; lower_hz: number; upper_hz: number }[];
        }>('bands.json');
        expect(BANDS.map((b) => [b.name, b.lowerHz, b.upperHz])).toEqual(
            bands.map((b) => [b.band, b.lower_hz, b.upper_hz])
        );
    });

    it.each([
        ['7', 7_000_000],
        [' 14.0745 ', 14_074_500],
        ['.475', 475_000],
        ['7.', 7_000_000],
        ['7.0410005', 7_041_001],
        ['', undefined],
        ['.', undefined],
        ['7,041', undefined],
        ['7.041MHz', undefined]
    ])('reads %j MHz as the server does', (mhz, hz) => {
        expect(frequencyHz(mhz)).toBe(hz);
    });

    it('finds the band of a frequency', () => {
        expect(bandOf('7.144')?.name).toBe('40m');
        expect(bandOf('54.000001')?.name).toBe('5m');
        expect(bandOf('7.5')).toBeUndefined();
    });

    it('keep each representative frequency in its band', () => {
        for (const b of BANDS) expect(bandOf(representativeFreq(b.name) ?? '')?.name).toBe(b.name);
        expect(representativeFreq('20M')).toBe('14.000');
        expect(representativeFreq('11m')).toBeUndefined();
    });
});

describe('modes', () => {
    it('offer only ADIF modes and submodes', () => {
        const { modes } = fixture<{ modes: { mode: string; submodes: string[] }[] }>('modes.json');
        for (const choice of MODE_CHOICES) {
            const mode = modes.find((m) => m.mode === choice.mode);
            expect(mode, choice.mode).toBeDefined();
            if (choice.submode !== '') expect(mode?.submodes).toContain(choice.submode);
        }
    });

    it('key a mode and its submode together', () => {
        expect(modeKey('ssb', 'lsb')).toBe('SSB|LSB');
        expect(parseModeKey('FT8|')).toEqual({ mode: 'FT8', submode: '' });
    });

    it('pick the sideband by band', () => {
        expect(sidebandFor('40m')).toBe('LSB');
        expect(sidebandFor('20m')).toBe('USB');
        expect(followsBand('SSB', 'LSB')).toBe(true);
        expect(followsBand('SSB', '')).toBe(false);
    });
});

describe('uuidv7', () => {
    it('puts the time first with the version and variant bits', () => {
        const id = uuidv7(0x0192f5a17c3e, new Uint8Array(10).fill(0xff));
        expect(id).toBe('0192f5a1-7c3e-7fff-bfff-ffffffffffff');
    });

    it('sorts by time', () => {
        expect(uuidv7(1000) < uuidv7(2000)).toBe(true);
    });
});

describe('format', () => {
    it('reads ADIF dates and times', () => {
        expect(isoDate('20261019')).toBe('2026-10-19');
        expect(hhmm('093112')).toBe('09:31');
        expect(hhmm('0931')).toBe('09:31');
        expect(utcFields(new Date('2026-10-19T09:31:12.345Z'))).toEqual({
            QSO_DATE: '20261019',
            TIME_ON: '093112'
        });
    });

    it('joins reports and parks', () => {
        expect(rst({ RST_SENT: '59', RST_RCVD: '57' })).toBe('59 / 57');
        expect(rst({})).toBe('');
        expect(parks({ MY_POTA_REF: 'JP-0012, JP-0345,' })).toEqual(['JP-0012', 'JP-0345']);
    });

    it('orders newest first, then by ID', () => {
        const row = (id: string, time: string) => ({
            id,
            fields: { QSO_DATE: '20261019', TIME_ON: time }
        });
        const rows = [row('a', '0900'), row('c', '0930'), row('b', '0930')];
        expect(rows.sort(compareNewestFirst).map((r) => r.id)).toEqual(['c', 'b', 'a']);
    });
});

describe('qsl', () => {
    it('reads absent as N', () => {
        expect(qslTone(undefined)).toBe('none');
        expect(qslTone('r')).toBe('warn');
        expect(qslTone('V')).toBe('ok');
        expect(qslLabel('q', true)).toBe('Queued');
    });
});

describe('contest', () => {
    it('expands the sent number', () => {
        expect(expandExchange('{serial}m', 7)).toBe('007M');
        expect(expandExchange('{serial:4}', 12)).toBe('0012');
        expect(usesSerial('1001H')).toBe(false);
        expect(exchangeString({ ...defaultContest(), includeRst: true }, '59', '007')).toBe(
            '59007'
        );
    });

    it('reports bad templates only when enabled', () => {
        const contest = { ...defaultContest(), exchangeTemplate: '{num}' };
        expect(contestProblem(contest)).toBeUndefined();
        expect(contestProblem({ ...contest, enabled: true })).toMatch(/only \{serial\}/);
        expect(contestProblem({ ...defaultContest(), enabled: true, nextSerial: 0 })).toMatch(
            /1 or more/
        );
    });
});
