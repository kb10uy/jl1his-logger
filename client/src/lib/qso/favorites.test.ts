import { describe, expect, it } from 'vitest';
import type { FavoriteFrequency } from '#lib/api/types.ts';
import {
    byFrequency,
    canSaveFavorite,
    favoriteBand,
    favoriteLabel,
    freqLabel,
    isFavorite
} from './favorites.ts';

const favorite = (freq: string, mode: string, submode = ''): FavoriteFrequency => ({
    id: `${freq}-${mode}-${submode}`,
    freq,
    mode,
    submode,
    created_at: '2026-10-28T00:00:00Z'
});

describe('favorite frequencies', () => {
    it('shows a frequency with at least three decimals', () => {
        expect(freqLabel('7.1')).toBe('7.100');
        expect(freqLabel('145')).toBe('145.000');
        expect(freqLabel('14.0745')).toBe('14.0745');
        expect(freqLabel('abc')).toBe('abc');
    });

    it('labels a favorite by its frequency and its submode or mode', () => {
        expect(favoriteLabel(favorite('7.1', 'SSB', 'LSB'))).toBe('7.100 LSB');
        expect(favoriteLabel(favorite('145', 'FM'))).toBe('145.000 FM');
        expect(favoriteBand(favorite('145', 'FM'))).toBe('2m');
        expect(favoriteBand(favorite('100', 'FM'))).toBe('—');
    });

    it('saves only a frequency in MHz with a mode', () => {
        expect(canSaveFavorite({ freq: '7.144', mode: 'SSB', submode: 'LSB' })).toBe(true);
        expect(canSaveFavorite({ freq: '', mode: 'SSB', submode: 'LSB' })).toBe(false);
        expect(canSaveFavorite({ freq: '0', mode: 'SSB', submode: '' })).toBe(false);
        expect(canSaveFavorite({ freq: '7.1x', mode: 'SSB', submode: '' })).toBe(false);
        expect(canSaveFavorite({ freq: '7.1', mode: '', submode: '' })).toBe(false);
    });

    it('matches the tuning by frequency, mode and submode', () => {
        const lsb = favorite('7.1', 'SSB', 'LSB');
        expect(isFavorite(lsb, { freq: '7.100', mode: 'SSB', submode: 'LSB' })).toBe(true);
        expect(isFavorite(lsb, { freq: '7.100', mode: 'SSB', submode: 'USB' })).toBe(false);
        expect(isFavorite(lsb, { freq: '7.101', mode: 'SSB', submode: 'LSB' })).toBe(false);
        expect(isFavorite(lsb, { freq: '', mode: 'SSB', submode: 'LSB' })).toBe(false);
    });

    it('sorts favorites by frequency, then mode', () => {
        const sorted = [
            favorite('145', 'FM'),
            favorite('7.1', 'SSB', 'USB'),
            favorite('14.074', 'FT8'),
            favorite('7.1', 'SSB', 'LSB'),
            favorite('7.1', 'CW')
        ].sort(byFrequency);
        expect(sorted.map(favoriteLabel)).toEqual([
            '7.100 CW',
            '7.100 LSB',
            '7.100 USB',
            '14.074 FT8',
            '145.000 FM'
        ]);
    });
});
