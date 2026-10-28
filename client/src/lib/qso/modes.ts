export interface ModeChoice {
    mode: string;
    submode: string;
    label: string;
}

export type ModeEntry = ModeChoice | { group: string; choices: ModeChoice[] };

const only = (mode: string): ModeChoice => ({ mode, submode: '', label: mode });

const group = (mode: string, submodes: string[]): ModeEntry => ({
    group: mode,
    choices: [only(mode), ...submodes.map((submode) => ({ mode, submode, label: submode }))]
});

export const MODE_ENTRIES: ModeEntry[] = [
    only('AM'),
    only('FM'),
    group('SSB', ['LSB', 'USB']),
    group('DIGITALVOICE', ['C4FM', 'DMR', 'DSTAR', 'FREEDV', 'M17']),
    group('CW', ['PCW']),
    only('FT8'),
    group('MFSK', ['FT4', 'FST4', 'FST4W', 'JS8', 'Q65', 'MFSK16', 'MFSK32']),
    group('RTTY', ['ASCI']),
    group('PSK', ['PSK31', 'PSK63', 'PSK125', 'QPSK31', 'QPSK63']),
    only('JT65'),
    only('JT9'),
    only('OLIVIA'),
    only('SSTV')
];

export const MODE_CHOICES: ModeChoice[] = MODE_ENTRIES.flatMap((entry) =>
    'group' in entry ? entry.choices : [entry]
);

export const modeKey = (mode: string, submode: string) =>
    `${mode.trim().toUpperCase()}|${submode.trim().toUpperCase()}`;

export function parseModeKey(key: string): { mode: string; submode: string } {
    const [mode, submode = ''] = key.split('|');
    return { mode, submode };
}

const LOWER_SIDEBAND = new Set(['2190m', '630m', '560m', '160m', '80m', '40m']);

export function sidebandFor(band: string): string {
    return LOWER_SIDEBAND.has(band.toLowerCase()) ? 'LSB' : 'USB';
}

export function followsBand(mode: string, submode: string): boolean {
    return mode === 'SSB' && (submode === 'LSB' || submode === 'USB');
}

const RST_MODES = new Set(['CW', 'RTTY', 'PSK', 'OLIVIA']);
const RST_SUBMODES = new Set(['MFSK16', 'MFSK32']);
const SNR_MODES = new Set(['FT8', 'MFSK', 'JT65', 'JT9']);

export function defaultRst(mode: string, submode = ''): string {
    if (RST_MODES.has(mode) || RST_SUBMODES.has(submode)) return '599';
    if (mode === 'SSTV') return '595';
    if (SNR_MODES.has(mode)) return '-10';
    return '59';
}
