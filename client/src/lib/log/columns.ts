import { m } from '#lib/paraglide/messages.js';
import { isAdifQsoField } from '#lib/qso/adif-fields.ts';

export interface Column {
    id: string;
    label: string;
    right?: boolean;
}

const builtIn = (id: string, label: () => string, right?: true): Column => ({
    id,
    get label() {
        return label();
    },
    ...(right && { right })
});

export const BUILT_IN_COLUMNS: readonly Column[] = [
    builtIn('time', m.field_date_utc),
    builtIn('call', m.field_call),
    builtIn('qsl', m.log_column_qsl),
    builtIn('lotw', m.log_column_lotw),
    builtIn('eqsl', m.log_column_eqsl),
    builtIn('band', m.field_band),
    builtIn('freq', m.field_freq_mhz, true),
    builtIn('mode', m.field_mode),
    builtIn('rst', m.log_column_rst),
    builtIn('txpwr', m.log_column_tx_power, true),
    builtIn('contest', m.field_contest),
    builtIn('number', m.field_number_sr),
    builtIn('station', m.field_station),
    builtIn('instrument', m.field_instrument),
    builtIn('park', m.log_column_park)
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
export const adifColumnId = (name: string) => `${ADIF_PREFIX}${name}`;

export function adifFieldOf(id: string): string | undefined {
    return id.startsWith(ADIF_PREFIX) ? id.slice(ADIF_PREFIX.length) : undefined;
}

export function adifColumnProblem(name: string): string | undefined {
    if (name === '') return m.log_column_enter_field();
    if (!isAdifQsoField(name)) return m.log_column_not_adif({ name });
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
