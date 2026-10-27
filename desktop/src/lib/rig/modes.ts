import { MODE_CHOICES, modeKey, type ModeChoice } from '#client/qso/modes.ts';

export const RIG_MODES = [
    'USB',
    'LSB',
    'PKTUSB',
    'PKTLSB',
    'CW',
    'FM',
    'AM',
    'RTTY',
    'C4FM'
] as const;

export type RigMode = (typeof RIG_MODES)[number];

export type RigFit = 'kept' | 'followed' | 'mismatch';

export type RigPrefs = Readonly<Record<string, string>>;

const ALIASES: Readonly<Record<string, RigMode>> = {
    CWR: 'CW',
    RTTYR: 'RTTY',
    FMN: 'FM',
    WFM: 'FM',
    AMS: 'AM',
    SAM: 'AM'
};

const DIGITAL_MODES = new Set(['FT8', 'MFSK', 'RTTY', 'PSK', 'JT65', 'JT9', 'OLIVIA', 'SSTV']);

const key = (c: ModeChoice) => modeKey(c.mode, c.submode);

const DIGITAL = MODE_CHOICES.filter(
    (c) => DIGITAL_MODES.has(c.mode) || (c.mode === 'DIGITALVOICE' && c.submode === 'FREEDV')
).map(key);

const SSB = [...MODE_CHOICES.filter((c) => c.mode === 'SSB').map(key), ...DIGITAL];

const VOICE = [
    'FM|',
    ...MODE_CHOICES.filter(
        (c) => c.mode === 'DIGITALVOICE' && ['C4FM', 'DSTAR', 'DMR', 'M17'].includes(c.submode)
    ).map(key)
];

const TABLE: Readonly<Record<RigMode, { fits: string[]; fallback: string }>> = {
    USB: { fits: SSB, fallback: 'SSB|USB' },
    LSB: { fits: SSB, fallback: 'SSB|LSB' },
    PKTUSB: { fits: DIGITAL, fallback: 'FT8|' },
    PKTLSB: { fits: DIGITAL, fallback: 'FT8|' },
    CW: { fits: ['CW|', 'CW|PCW'], fallback: 'CW|' },
    FM: { fits: VOICE, fallback: 'FM|' },
    AM: { fits: ['AM|'], fallback: 'AM|' },
    RTTY: { fits: ['RTTY|', 'RTTY|ASCI'], fallback: 'RTTY|' },
    C4FM: { fits: ['DIGITALVOICE|C4FM'], fallback: 'DIGITALVOICE|C4FM' }
};

export function rigModeOf(reported: string): RigMode | undefined {
    const mode = reported.trim().toUpperCase();
    return (RIG_MODES as readonly string[]).includes(mode) ? (mode as RigMode) : ALIASES[mode];
}

export function fitting(rig: RigMode): string[] {
    return TABLE[rig].fits;
}

export function loggedFor(rig: RigMode, prefs: RigPrefs): string {
    const preferred = prefs[rig];
    return preferred !== undefined && TABLE[rig].fits.includes(preferred)
        ? preferred
        : TABLE[rig].fallback;
}

export function followRig(
    rig: RigMode | undefined,
    current: string,
    prefs: RigPrefs
): { key: string; fit: RigFit } {
    if (rig === undefined) return { key: current, fit: 'kept' };
    if ((rig === 'USB' || rig === 'LSB') && current.startsWith('SSB|')) {
        return { key: `SSB|${rig}`, fit: 'kept' };
    }
    if (TABLE[rig].fits.includes(current)) return { key: current, fit: 'kept' };
    return { key: loggedFor(rig, prefs), fit: 'followed' };
}

export function pickMode(
    rig: RigMode | undefined,
    picked: string,
    prefs: RigPrefs
): { fit: RigFit; prefs: RigPrefs } {
    if (rig === undefined) return { fit: 'kept', prefs };
    const fits = TABLE[rig].fits;
    if (!fits.includes(picked)) return { fit: 'mismatch', prefs };
    if (fits.length === 1 || prefs[rig] === picked) return { fit: 'kept', prefs };
    return { fit: 'kept', prefs: { ...prefs, [rig]: picked } };
}
