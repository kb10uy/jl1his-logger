import { bandOf, representativeFreq } from './bands.ts';
import { defaultRst, followsBand, sidebandFor } from './modes.ts';

export interface Tuning {
    freq: string;
    band: string;
    mode: string;
    submode: string;
}

export interface Reports {
    rstSent: string;
    rstRcvd: string;
}

function movedTo<T extends Tuning>(tuning: T, band: string): T {
    const submode =
        band !== tuning.band && followsBand(tuning.mode, tuning.submode)
            ? sidebandFor(band)
            : tuning.submode;
    return { ...tuning, band, submode };
}

export function withBand<T extends Tuning>(tuning: T, band: string): T {
    return movedTo({ ...tuning, freq: representativeFreq(band) ?? tuning.freq }, band);
}

export function withFreq<T extends Tuning>(tuning: T, freq: string): T {
    return movedTo({ ...tuning, freq }, bandOf(freq)?.name ?? tuning.band);
}

export function withMode<T extends Tuning & Reports>(tuning: T, mode: string, submode: string): T {
    const before = defaultRst(tuning.mode);
    const after = defaultRst(mode);
    return {
        ...tuning,
        mode,
        submode,
        rstSent: tuning.rstSent === before ? after : tuning.rstSent,
        rstRcvd: tuning.rstRcvd === before ? after : tuning.rstRcvd
    };
}
