import { readdirSync, readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';

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
});
