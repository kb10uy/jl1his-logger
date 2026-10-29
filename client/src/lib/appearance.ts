import { compact, load, save, theme, type Theme } from '#lib/device.ts';

export function applyTheme(value: Theme | null) {
    if (value === null) delete document.documentElement.dataset.theme;
    else document.documentElement.dataset.theme = value;
}

export function chooseTheme(value: Theme | null) {
    applyTheme(value);
    save(theme, value);
}

export function applyCompact(value: boolean) {
    if (value) document.documentElement.dataset.compact = '';
    else delete document.documentElement.dataset.compact;
}

export function chooseCompact(value: boolean) {
    applyCompact(value);
    save(compact, value);
}

export const storedTheme = () => load(theme);

export const storedCompact = () => load(compact);
