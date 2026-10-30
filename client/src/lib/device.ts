import { defaultEntry, type EntrySettings } from '#lib/entry/settings.ts';
import { DEFAULT_COLUMNS, normalizeColumns } from '#lib/log/columns.ts';
import { defaultContest } from '#lib/qso/contest.ts';
import { defaultVoice, type VoiceSettings } from '#lib/voice/settings.ts';

export const THEMES = ['light', 'dark'] as const;

export type Theme = (typeof THEMES)[number];

export function asTheme(value: string | null | undefined): Theme | null {
    return THEMES.find((t) => t === value) ?? null;
}

export interface DeviceSetting<T> {
    readonly key: string;
    read(stored: string | null): T;
    write(value: T): string | null;
}

export function load<T>(setting: DeviceSetting<T>): T {
    let stored: string | null = null;
    try {
        stored = localStorage.getItem(setting.key);
    } catch {
        stored = null;
    }
    try {
        return setting.read(stored);
    } catch {
        return setting.read(null);
    }
}

export function save<T>(setting: DeviceSetting<T>, value: T) {
    try {
        const stored = setting.write(value);
        if (stored === null) localStorage.removeItem(setting.key);
        else localStorage.setItem(setting.key, stored);
    } catch {
        return;
    }
}

export function flag(key: string): DeviceSetting<boolean> {
    return {
        key,
        read: (stored) => stored === '1' || stored === 'true',
        write: (value) => (value ? '1' : null)
    };
}

export function text(key: string): DeviceSetting<string | null> {
    return { key, read: (stored) => stored, write: (value) => value };
}

export function json<T>(key: string, read: (stored: Partial<T>) => T): DeviceSetting<T> {
    return {
        key,
        read: (stored) => read(stored === null ? {} : (JSON.parse(stored) as Partial<T>)),
        write: (value) => JSON.stringify(value)
    };
}

const KEY = 'jl1his-logger';

export const theme: DeviceSetting<Theme | null> = {
    key: `${KEY}.theme`,
    read: asTheme,
    write: (value) => value
};

export const compact = flag(`${KEY}.compact`);

export const sidebarCollapsed = flag(`${KEY}.sidebar-collapsed`);

export const lastLogbook = text(`${KEY}.logbook`);

export const logColumns: DeviceSetting<string[]> = {
    key: `${KEY}.log-columns`,
    read: (stored) =>
        stored === null ? [...DEFAULT_COLUMNS] : normalizeColumns(JSON.parse(stored)),
    write: (value) => JSON.stringify(value)
};

export const voice = json<VoiceSettings>(`${KEY}.voice`, (stored) => {
    const defaults = defaultVoice(globalThis.navigator?.language);
    return {
        ...defaults,
        ...stored,
        language:
            stored.language === 'ja' || stored.language === 'en'
                ? stored.language
                : defaults.language
    };
});

export function withEntryDefaults(stored: Partial<EntrySettings>): EntrySettings {
    return {
        ...defaultEntry(),
        ...stored,
        contest: { ...defaultContest(), ...stored.contest }
    };
}

export const entry = (logbookId: string) =>
    json<EntrySettings>(`${KEY}.entry.${logbookId}`, withEntryDefaults);
