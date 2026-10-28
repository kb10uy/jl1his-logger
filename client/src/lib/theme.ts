export const THEMES = ['light', 'dark'] as const;

export type Theme = (typeof THEMES)[number];

const KEY = 'jl1his-logger.theme';

export function asTheme(value: string | null | undefined): Theme | null {
    return THEMES.find((t) => t === value) ?? null;
}

export function applyTheme(theme: Theme | null) {
    if (theme === null) delete document.documentElement.dataset.theme;
    else document.documentElement.dataset.theme = theme;
}

export function storedTheme(): Theme | null {
    try {
        return asTheme(localStorage.getItem(KEY));
    } catch {
        return null;
    }
}

export function chooseTheme(theme: Theme | null) {
    applyTheme(theme);
    try {
        if (theme === null) localStorage.removeItem(KEY);
        else localStorage.setItem(KEY, theme);
    } catch {
        return;
    }
}
