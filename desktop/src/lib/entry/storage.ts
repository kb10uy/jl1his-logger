import type { Instrument, Station } from '#client/api/types.ts';
import { json, load, save, withEntryDefaults } from '#client/device.ts';
import type { EntrySettings } from '#client/entry/settings.ts';
import type { Logbook } from '#lib/session/types.ts';

export interface Cache {
    logbooks: Logbook[];
    stations: Record<string, Station[]>;
    instruments: Record<string, Instrument[]>;
}

const settings = json<EntrySettings>('jl1his-logger-desktop.entry', withEntryDefaults);

const cache = json<Cache>('jl1his-logger-desktop.logbooks', (stored) => ({
    logbooks: stored.logbooks ?? [],
    stations: stored.stations ?? {},
    instruments: stored.instruments ?? {}
}));

export const loadSettings = () => load(settings);

export const saveSettings = (value: EntrySettings) => save(settings, value);

export const loadCache = () => load(cache);

export const saveCache = (value: Cache) => save(cache, value);
