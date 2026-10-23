import type { QueryToken } from './index.ts';

export type ChipTone = 'normal' | 'negated' | 'error' | 'pending';

export interface Chip {
    label: string;
    op: string;
    value: string;
    tone: ChipTone;
}

const NAMES: Record<string, string> = {
    call: 'Call',
    base: 'Base call',
    band: 'Band',
    mode: 'Mode',
    contest: 'Contest',
    station: 'Station',
    park: 'My park',
    sent: 'QSL sent',
    rcvd: 'QSL rcvd',
    date: 'Date',
    dxcc: 'DXCC'
};

const FIELD_NAMES: Record<string, string> = {
    MY_POTA_REF: 'My park',
    QSL_SENT: 'QSL sent',
    QSL_RCVD: 'QSL rcvd',
    CONTEST_ID: 'Contest'
};

const ERRORS: Record<string, string> = {
    unknown_field: 'unknown field',
    bad_field_name: 'bad field name',
    bad_date: 'bad date',
    unclosed_quote: 'open quote',
    too_many_values: 'too many values',
    empty: 'empty'
};

function dateChip(value: string): { op: string; value: string } {
    const range = value.split('..');
    if (range.length === 2) {
        const [from, until] = range;
        if (from && until) return { op: 'between', value: `${from} – ${until}` };
        if (from) return { op: 'from', value: from };
        return { op: 'until', value: until };
    }
    for (const [prefix, op] of [
        ['>=', 'from'],
        ['<=', 'until'],
        ['>', 'after'],
        ['<', 'before']
    ] as const) {
        if (value.startsWith(prefix)) return { op, value: value.slice(prefix.length) };
    }
    return { op: 'on', value };
}

function label(token: QueryToken): string {
    if (token.key === 'adif') return `ADIF ${token.field ?? ''}`.trim();
    return NAMES[token.key ?? ''] ?? token.key ?? 'Text';
}

export function chip(token: QueryToken): Chip {
    if (token.error?.code === 'missing_value') {
        return { label: label(token), op: 'is', value: '…', tone: 'pending' };
    }
    if (token.error) {
        return {
            label: token.text,
            op: ERRORS[token.error.code] ?? token.error.code,
            value: '',
            tone: 'error'
        };
    }
    const tone: ChipTone = token.negated ? 'negated' : 'normal';
    if (token.key === null) {
        return {
            label: 'Text',
            op: token.negated ? 'excludes' : 'contains',
            value: `"${token.values[0]}"`,
            tone
        };
    }
    if (token.key === 'has' || token.key === 'no') {
        const present = (token.key === 'has') !== token.negated;
        const field = token.field ?? '';
        return {
            label: FIELD_NAMES[field] ?? field,
            op: present ? 'is present' : 'is missing',
            value: '',
            tone: 'normal'
        };
    }
    if (token.key === 'date') {
        const { op, value } = dateChip(token.values[0]);
        return { label: 'Date', op: token.negated ? `not ${op}` : op, value, tone };
    }
    const multi = token.values.length > 1;
    const wildcard = token.key === 'call' && token.values.some((v) => /[*?]/.test(v));
    const op = token.negated
        ? multi
            ? 'is none of'
            : 'is not'
        : wildcard
          ? 'matches'
          : multi
            ? 'is any of'
            : 'is';
    return { label: label(token), op, value: token.values.join(', '), tone };
}

export interface Template {
    label: string;
    term: string;
}

export function templates(today: string): Template[] {
    return [
        { label: 'Call', term: 'call:' },
        { label: 'Band', term: 'band:' },
        { label: 'Mode', term: 'mode:' },
        { label: 'QSL sent', term: 'sent:' },
        { label: 'QSL received', term: 'rcvd:' },
        { label: 'Station', term: 'station:' },
        { label: 'My park', term: 'park:' },
        { label: 'Date from', term: `date:${today}..` },
        { label: 'FREQ missing', term: 'no:freq' },
        { label: 'ADIF field', term: 'adif.TX_PWR:' }
    ];
}

export const SYNTAX: [string, string][] = [
    ['call:JH1*', 'wildcard, * and ?'],
    ['band:40m,20m', 'comma = any of'],
    ['-mode:FT8', 'leading - excludes'],
    ['date:2026-10-01..', 'range, either end open'],
    ['date:>=2026-10-18', 'also > < <='],
    ['sent:R,Q rcvd:N', 'QSL_SENT / QSL_RCVD'],
    ['station:"JL1HIS/1"', 'quote values with spaces'],
    ['park:JP-0012', 'any MY_POTA_REF'],
    ['no:freq has:park', 'missing / present'],
    ['adif.TX_PWR:10', 'any other ADIF field'],
    ['dxcc:339,291', 'DXCC entity code'],
    ['yokohama', 'bare word: call, name, QTH, comment']
];
