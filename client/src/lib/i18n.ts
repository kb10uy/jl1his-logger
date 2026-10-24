import { m } from '#lib/paraglide/messages.js';
import {
    getLocale,
    localStorageKey,
    locales,
    setLocale,
    type Locale
} from '#lib/paraglide/runtime.js';

export const SLOT = '\u0000';

export function slots(text: string): string[] {
    return text.split(SLOT);
}

export function formatNumber(n: number): string {
    return new Intl.NumberFormat(getLocale()).format(n);
}

export function roleLabel(role: string): string {
    switch (role) {
        case 'admin':
            return m.role_admin();
        case 'user':
            return m.role_user();
        case 'owner':
            return m.role_owner();
        case 'operator':
            return m.role_operator();
        default:
            return role;
    }
}

export const LOCALE_NAMES: Readonly<Record<Locale, string>> = {
    en: 'English',
    ja: '日本語'
};

export { locales, type Locale };

export function storedLocale(): Locale | null {
    try {
        const stored = localStorage.getItem(localStorageKey);
        return locales.find((l) => l === stored) ?? null;
    } catch {
        return null;
    }
}

export function chooseLocale(locale: Locale | null) {
    if (locale !== null) {
        setLocale(locale);
        return;
    }
    try {
        localStorage.removeItem(localStorageKey);
    } catch {
        return;
    }
    location.reload();
}
