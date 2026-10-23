import { DEFAULT_COLUMNS, normalizeColumns } from '#lib/log/columns.ts';

const KEY = 'jl1his-logger.log-columns';

export function loadColumns(): string[] {
    try {
        const stored = localStorage.getItem(KEY);
        return stored === null ? [...DEFAULT_COLUMNS] : normalizeColumns(JSON.parse(stored));
    } catch {
        return [...DEFAULT_COLUMNS];
    }
}

export function saveColumns(ids: readonly string[]) {
    try {
        localStorage.setItem(KEY, JSON.stringify(ids));
    } catch {
        return;
    }
}
