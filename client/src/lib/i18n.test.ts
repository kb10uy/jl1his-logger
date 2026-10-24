import { readdirSync, readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';
import { errorText } from './api/errors.ts';
import { m } from './paraglide/messages.js';

const keys = (locale: string, file: string): string[] =>
    Object.keys(
        JSON.parse(
            readFileSync(new URL(`../../messages/${locale}/${file}`, import.meta.url), 'utf8')
        )
    ).sort();

describe('messages', () => {
    it.each(readdirSync(new URL('../../messages/en/', import.meta.url)))(
        'has every key of %s in Japanese',
        (file) => {
            expect(keys('ja', file)).toEqual(keys('en', file));
        }
    );

    it('formats counts with plurals and grouping', () => {
        expect(m.common_qso_count({ count: 1 })).toBe('1 QSO');
        expect(m.common_qso_count({ count: 12345 })).toBe('12,345 QSOs');
        expect(m.common_qso_count({ count: 12345 }, { locale: 'ja' })).toBe('12,345 件の QSO');
    });
});

describe('errorText', () => {
    it('words an error from its code and parameters', () => {
        expect(errorText({ code: 'qso_invalid', field: 'BAND', value: '41m' })).toBe(
            'BAND "41m" is not valid.'
        );
        expect(errorText({ code: 'not_found', what: 'QSO' })).toBe('The QSO was not found.');
        expect(errorText({ code: 'unknown_field', term: 'qrg:70', value: 'qrg' })).toBe(
            'qrg is not a search field.'
        );
    });

    it('leaves an unknown code or noun to the server text', () => {
        expect(errorText({ code: 'something_new', error: 'something new' })).toBeUndefined();
        expect(errorText({ code: 'not_found', what: 'widget' })).toBeUndefined();
        expect(errorText({ error: 'no code' })).toBeUndefined();
    });
});
