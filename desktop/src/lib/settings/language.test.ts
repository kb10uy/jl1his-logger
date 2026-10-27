import { describe, expect, it } from 'vitest';
import { asLocale, systemLanguage } from './language.ts';

describe('asLocale', () => {
    it('keeps only the app locales', () => {
        expect(asLocale('ja')).toBe('ja');
        expect(asLocale('fr')).toBeNull();
        expect(asLocale(null)).toBeNull();
    });
});

describe('systemLanguage', () => {
    it.each([
        [['ja-JP', 'en-US'], 'ja'],
        [['fr-FR', 'en-GB'], 'en'],
        [['de'], 'en'],
        [[], 'en']
    ])('picks the first supported of %j', (preferred, expected) => {
        expect(systemLanguage(preferred)).toBe(expected);
    });
});
