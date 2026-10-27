import { defineCustomClientStrategy, locales, type Locale } from '#lib/paraglide/runtime.js';

const KEY = 'jl1his-logger-desktop.language';

export const LOCALE_NAMES: Readonly<Record<Locale, string>> = {
    en: 'English',
    ja: '日本語'
};

export function asLocale(value: string | null | undefined): Locale | null {
    return locales.find((l) => l === value) ?? null;
}

export function storedLanguage(): Locale | null {
    try {
        return asLocale(localStorage.getItem(KEY));
    } catch {
        return null;
    }
}

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

export function applyLanguage(language: Locale | null): boolean {
    if (storedLanguage() === language) return false;
    try {
        if (language === null) localStorage.removeItem(KEY);
        else localStorage.setItem(KEY, language);
    } catch {
        return false;
    }
    location.reload();
    return true;
}
