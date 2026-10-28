import type { FavoriteFrequency, FrequencyInput } from '#lib/api/types.ts';
import { bandOf, formatMhz, frequencyHz } from './bands.ts';

export function freqLabel(freq: string): string {
    const hz = frequencyHz(freq);
    return hz === undefined ? freq : formatMhz(hz);
}

export function favoriteLabel(tuning: FrequencyInput): string {
    return `${freqLabel(tuning.freq)} ${tuning.submode || tuning.mode}`;
}

export function favoriteBand(favorite: FrequencyInput): string {
    return bandOf(favorite.freq)?.name ?? '—';
}

export function canSaveFavorite(tuning: FrequencyInput): boolean {
    const hz = frequencyHz(tuning.freq);
    return hz !== undefined && hz > 0 && tuning.mode.trim() !== '';
}

export function isFavorite(favorite: FrequencyInput, tuning: FrequencyInput): boolean {
    const hz = frequencyHz(tuning.freq);
    return (
        hz !== undefined &&
        frequencyHz(favorite.freq) === hz &&
        favorite.mode.toUpperCase() === tuning.mode.trim().toUpperCase() &&
        favorite.submode.toUpperCase() === tuning.submode.trim().toUpperCase()
    );
}

export function byFrequency(a: FavoriteFrequency, b: FavoriteFrequency): number {
    return (
        (frequencyHz(a.freq) ?? 0) - (frequencyHz(b.freq) ?? 0) ||
        a.mode.localeCompare(b.mode) ||
        a.submode.localeCompare(b.submode)
    );
}
