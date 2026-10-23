import { ADIF_QSO_FIELDS } from '#lib/qso/adif-fields.ts';

export interface Column {
    id: string;
    label: string;
    right?: boolean;
}

export const BUILT_IN_COLUMNS: readonly Column[] = [
    { id: 'time', label: 'Date / UTC' },
    { id: 'call', label: 'Call' },
    { id: 'qsl', label: 'QSL S / R' },
    { id: 'lotw', label: 'LoTW S / R' },
    { id: 'eqsl', label: 'eQSL S / R' },
    { id: 'band', label: 'Band' },
    { id: 'freq', label: 'Freq MHz', right: true },
    { id: 'mode', label: 'Mode' },
    { id: 'rst', label: 'RST S / R' },
    { id: 'txpwr', label: 'TX Power W', right: true },
    { id: 'contest', label: 'Contest' },
    { id: 'number', label: 'Number S / R' },
    { id: 'station', label: 'Station' },
    { id: 'park', label: 'Park' }
];

export const DEFAULT_COLUMNS: readonly string[] = [
    'time',
    'call',
    'qsl',
    'band',
    'freq',
    'mode',
    'rst',
    'station',
    'park'
];

const ADIF_PREFIX = 'adif.';
const KNOWN = new Set(ADIF_QSO_FIELDS);

export const adifColumnId = (name: string) => `${ADIF_PREFIX}${name}`;

export function adifFieldOf(id: string): string | undefined {
    return id.startsWith(ADIF_PREFIX) ? id.slice(ADIF_PREFIX.length) : undefined;
}

export function adifColumnProblem(name: string): string | undefined {
    if (name === '') return 'Enter an ADIF field name.';
    if (!KNOWN.has(name) && !/^APP_[A-Z0-9_]+$/.test(name))
        return `${name} is not an ADIF QSO field.`;
    return undefined;
}

export function column(id: string): Column | undefined {
    const name = adifFieldOf(id);
    if (name !== undefined)
        return adifColumnProblem(name) === undefined ? { id, label: name } : undefined;
    return BUILT_IN_COLUMNS.find((c) => c.id === id);
}

export function normalizeColumns(stored: unknown): string[] {
    if (!Array.isArray(stored)) return [...DEFAULT_COLUMNS];
    const ids = [
        ...new Set(stored.filter((id): id is string => typeof id === 'string' && !!column(id)))
    ];
    return ids.length > 0 ? ids : [...DEFAULT_COLUMNS];
}

export function moveColumn(ids: readonly string[], from: number, to: number): string[] {
    if (from < 0 || from >= ids.length || to < 0 || to >= ids.length) return [...ids];
    const next = [...ids];
    const [moved] = next.splice(from, 1);
    next.splice(to, 0, moved);
    return next;
}
