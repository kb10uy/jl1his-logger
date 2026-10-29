import { load, save, type DeviceSetting } from '#client/device.ts';
import { defineCustomClientStrategy, locales, type Locale } from '#lib/paraglide/runtime.js';

export const LOCALE_NAMES: Readonly<Record<Locale, string>> = {
    en: 'English',
    ja: '日本語'
};

export function asLocale(value: string | null | undefined): Locale | null {
    return locales.find((l) => l === value) ?? null;
}

const language: DeviceSetting<Locale | null> = {
    key: 'jl1his-logger-desktop.language',
    read: asLocale,
    write: (value) => value
};

export const storedLanguage = () => load(language);

export function systemLanguage(preferred: readonly string[]): Locale {
    for (const tag of preferred) {
        const found = asLocale(tag.toLowerCase().split('-')[0]);
        if (found !== null) return found;
    }
    return 'en';
}

export function defineLanguageStrategy() {
    defineCustomClientStrategy('custom-config', {
        getLocale: () => storedLanguage() ?? undefined,
        setLocale: () => undefined
    });
}

export function applyLanguage(value: Locale | null): boolean {
    if (storedLanguage() === value) return false;
    save(language, value);
    location.reload();
    return true;
}
