export interface QsoMode {
    mode: string;
    submode?: string;
}

export const KNOWN_MODES = {
    FM: { mode: 'FM' },
    AM: { mode: 'AM' },
    SSB: { mode: 'SSB' },
    USB: { mode: 'SSB', submode: 'USB' },
    LSB: { mode: 'SSB', submode: 'LSB' },
    CW: { mode: 'CW' },
    RTTY: { mode: 'RTTY' },
    FT8: { mode: 'FT8' },
    FT4: { mode: 'MFSK', submode: 'FT4' },
    PSK31: { mode: 'PSK', submode: 'PSK31' },
    JT65: { mode: 'JT65' },
    SSTV: { mode: 'SSTV' },
    DSTAR: { mode: 'DIGITALVOICE', submode: 'DSTAR' },
    C4FM: { mode: 'DIGITALVOICE', submode: 'C4FM' },
    DMR: { mode: 'DIGITALVOICE', submode: 'DMR' }
} as const satisfies Record<string, QsoMode>;

export type KnownModeName = keyof typeof KNOWN_MODES;

export function isKnownModeName(name: string): name is KnownModeName {
    return Object.hasOwn(KNOWN_MODES, name);
}

export function modeOf(name: string): QsoMode {
    const upper = name.trim().toUpperCase();
    return isKnownModeName(upper) ? { ...KNOWN_MODES[upper] } : { mode: upper };
}

export function modeLabel({ mode, submode }: QsoMode): { label: string; detail?: string } {
    return submode === undefined ? { label: mode } : { label: submode, detail: mode };
}
