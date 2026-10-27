import type { Instrument, Station } from '#client/api/types.ts';
import { defaultEntry, type EntrySettings } from '#client/entry/settings.ts';
import { defaultContest } from '#client/qso/contest.ts';
import type { Logbook } from '#lib/session/types.ts';

const SETTINGS = 'jl1his-logger-desktop.entry';
const CACHE = 'jl1his-logger-desktop.logbooks';

export interface Cache {
    logbooks: Logbook[];
    stations: Record<string, Station[]>;
    instruments: Record<string, Instrument[]>;
}

function read<T>(key: string): Partial<T> {
    try {
        const stored = localStorage.getItem(key);
        return stored === null ? {} : (JSON.parse(stored) as Partial<T>);
    } catch {
        return {};
    }
}

function write(key: string, value: unknown) {
    try {
        localStorage.setItem(key, JSON.stringify(value));
    } catch {
        return;
    }
}

export function loadSettings(): EntrySettings {
    const stored = read<EntrySettings>(SETTINGS);
    return {
        ...defaultEntry(),
        ...stored,
        contest: { ...defaultContest(), ...stored.contest }
    };
}

export const saveSettings = (settings: EntrySettings) => write(SETTINGS, settings);

export function loadCache(): Cache {
    const stored = read<Cache>(CACHE);
    return {
        logbooks: stored.logbooks ?? [],
        stations: stored.stations ?? {},
        instruments: stored.instruments ?? {}
    };
}

export const saveCache = (cache: Cache) => write(CACHE, cache);
