import { defaultContest, type ContestSettings } from '#lib/qso/contest.ts';

export interface EntrySettings {
    stationId: string;
    freq: string;
    band: string;
    mode: string;
    submode: string;
    contest: ContestSettings;
}

export function defaultEntry(): EntrySettings {
    return {
        stationId: '',
        freq: '',
        band: '40m',
        mode: 'SSB',
        submode: 'LSB',
        contest: defaultContest()
    };
}

const key = (logbookId: string) => `jl1his-logger.entry.${logbookId}`;

export function loadEntry(logbookId: string): EntrySettings {
    try {
        const stored = localStorage.getItem(key(logbookId));
        if (stored === null) return defaultEntry();
        const parsed = JSON.parse(stored) as Partial<EntrySettings>;
        return {
            ...defaultEntry(),
            ...parsed,
            contest: { ...defaultContest(), ...parsed.contest }
        };
    } catch {
        return defaultEntry();
    }
}

export function saveEntry(logbookId: string, settings: EntrySettings) {
    try {
        localStorage.setItem(key(logbookId), JSON.stringify(settings));
    } catch {
        return;
    }
}
