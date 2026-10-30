import { m } from '#lib/paraglide/messages.js';
import type { QueryToken } from './index.ts';

export type ChipTone = 'normal' | 'negated' | 'error' | 'pending';

export interface Chip {
    label: string;
    op: string;
    value: string;
    tone: ChipTone;
}

type Words = () => string;

const NAMES: Record<string, Words> = {
    call: m.field_call,
    base: m.log_chip_base_call,
    band: m.field_band,
    mode: m.field_mode,
    contest: m.field_contest,
    station: m.field_station,
    'station-id': m.field_station,
    instrument: m.field_instrument,
    'instrument-id': m.field_instrument,
    park: m.field_park,
    sent: m.field_qsl_sent,
    rcvd: m.field_qsl_rcvd,
    'lotw-sent': m.log_chip_lotw_sent,
    'lotw-rcvd': m.log_chip_lotw_rcvd,
    'eqsl-sent': m.log_chip_eqsl_sent,
    'eqsl-rcvd': m.log_chip_eqsl_rcvd,
    date: m.field_date,
    freq: m.log_chip_freq,
    pwr: m.field_tx_power,
    dxcc: () => 'DXCC'
};

const FIELD_NAMES: Record<string, Words> = {
    MY_POTA_REF: m.field_park,
    QSL_SENT: m.field_qsl_sent,
    QSL_RCVD: m.field_qsl_rcvd,
    CONTEST_ID: m.field_contest,
    LOTW_QSL_SENT: m.log_chip_lotw_sent,
    LOTW_QSL_RCVD: m.log_chip_lotw_rcvd,
    EQSL_QSL_SENT: m.log_chip_eqsl_sent,
    EQSL_QSL_RCVD: m.log_chip_eqsl_rcvd,
    TX_PWR: m.field_tx_power
};

const ERRORS: Record<string, Words> = {
    unknown_field: m.log_chip_error_unknown_field,
    bad_field_name: m.log_chip_error_bad_field_name,
    bad_date: m.log_chip_error_bad_date,
    bad_number: m.log_chip_error_bad_number,
    unclosed_quote: m.log_chip_error_unclosed_quote,
    too_many_values: m.log_chip_error_too_many_values,
    empty: m.log_chip_error_empty
};

type Op = readonly [plain: Words, negated: Words];

interface RangeWords {
    from: Op;
    until: Op;
    after: Op;
    before: Op;
    exact: Op;
    unit: string;
}

const BETWEEN: Op = [m.log_chip_between, m.log_chip_not_between];

const NUMBER_WORDS: Omit<RangeWords, 'unit'> = {
    from: [m.log_chip_at_least, m.log_chip_not_at_least],
    until: [m.log_chip_at_most, m.log_chip_not_at_most],
    after: [m.log_chip_above, m.log_chip_not_above],
    before: [m.log_chip_below, m.log_chip_not_below],
    exact: [m.log_chip_number_is, m.log_chip_number_not_is]
};

const RANGES: Record<string, RangeWords> = {
    date: {
        from: [m.log_chip_date_from, m.log_chip_date_not_from],
        until: [m.log_chip_date_until, m.log_chip_date_not_until],
        after: [m.log_chip_date_after, m.log_chip_date_not_after],
        before: [m.log_chip_date_before, m.log_chip_date_not_before],
        exact: [m.log_chip_date_on, m.log_chip_date_not_on],
        unit: ''
    },
    freq: { ...NUMBER_WORDS, unit: ' MHz' },
    pwr: { ...NUMBER_WORDS, unit: ' W' }
};

function rangeChip(value: string, words: RangeWords): { op: Op; value: string } {
    const range = value.split('..');
    if (range.length === 2) {
        const [from, until] = range;
        if (from && until) return { op: BETWEEN, value: `${from} – ${until}${words.unit}` };
        if (from) return { op: words.from, value: `${from}${words.unit}` };
        return { op: words.until, value: `${until}${words.unit}` };
    }
    for (const [prefix, op] of [
        ['>=', words.from],
        ['<=', words.until],
        ['>', words.after],
        ['<', words.before]
    ] as const) {
        if (value.startsWith(prefix))
            return { op, value: `${value.slice(prefix.length)}${words.unit}` };
    }
    return { op: words.exact, value: `${value}${words.unit}` };
}

function label(token: QueryToken): string {
    if (token.key === 'adif') return `ADIF ${token.field ?? ''}`.trim();
    return NAMES[token.key ?? '']?.() ?? token.key ?? m.log_chip_text();
}

export function chip(token: QueryToken): Chip {
    if (token.error?.code === 'missing_value') {
        return { label: label(token), op: m.log_chip_is(), value: '…', tone: 'pending' };
    }
    if (token.error) {
        return {
            label: token.text,
            op: ERRORS[token.error.code]?.() ?? token.error.code,
            value: '',
            tone: 'error'
        };
    }
    const tone: ChipTone = token.negated ? 'negated' : 'normal';
    if (token.key === null) {
        return {
            label: m.log_chip_text(),
            op: token.negated ? m.log_chip_excludes() : m.log_chip_contains(),
            value: `"${token.values[0]}"`,
            tone
        };
    }
    if (token.key === 'has' || token.key === 'no') {
        const present = (token.key === 'has') !== token.negated;
        const field = token.field ?? '';
        return {
            label: FIELD_NAMES[field]?.() ?? field,
            op: present ? m.log_chip_is_present() : m.log_chip_is_missing(),
            value: '',
            tone: 'normal'
        };
    }
    const words = RANGES[token.key];
    if (words) {
        const { op, value } = rangeChip(token.values[0], words);
        return { label: label(token), op: op[token.negated ? 1 : 0](), value, tone };
    }
    const multi = token.values.length > 1;
    const wildcard = token.key === 'call' && token.values.some((v) => /[*?]/.test(v));
    const op = token.negated
        ? multi
            ? m.log_chip_is_none_of()
            : m.log_chip_is_not()
        : wildcard
          ? m.log_chip_matches()
          : multi
            ? m.log_chip_is_any_of()
            : m.log_chip_is();
    return { label: label(token), op, value: token.values.join(', '), tone };
}

export interface Template {
    label: string;
    term: string;
}

export function templates(today: string): Template[] {
    return [
        { label: m.field_call(), term: 'call:' },
        { label: m.field_band(), term: 'band:' },
        { label: m.field_mode(), term: 'mode:' },
        { label: m.field_qsl_sent(), term: 'sent:' },
        { label: m.log_qsl_received(), term: 'rcvd:' },
        { label: m.log_chip_lotw_sent(), term: 'lotw-sent:' },
        { label: m.log_template_lotw_received(), term: 'lotw-rcvd:' },
        { label: m.log_chip_eqsl_sent(), term: 'eqsl-sent:' },
        { label: m.log_template_eqsl_received(), term: 'eqsl-rcvd:' },
        { label: m.field_station(), term: 'station:' },
        { label: m.field_instrument(), term: 'instrument:' },
        { label: m.field_park(), term: 'park:' },
        { label: m.log_template_date_from(), term: `date:${today}..` },
        { label: m.field_freq_mhz(), term: 'freq:' },
        { label: m.log_template_tx_power_w(), term: 'pwr:' },
        { label: m.log_template_freq_missing(), term: 'no:freq' },
        { label: m.log_adif_field(), term: 'adif.GRIDSQUARE:' }
    ];
}

export const SYNTAX: readonly [example: string, meaning: Words][] = [
    ['call:JH1*', m.log_syntax_wildcard],
    ['band:40m,20m', m.log_syntax_any_of],
    ['-mode:FT8', m.log_syntax_exclude],
    ['date:2026-10-01..', m.log_syntax_range],
    ['date:>=2026-10-18', m.log_syntax_compare],
    ['sent:R,Q rcvd:N', () => 'QSL_SENT / QSL_RCVD'],
    ['lotw-sent:Y eqsl-rcvd:Y', m.log_syntax_lotw_eqsl],
    ['freq:14.0..14.35', m.log_syntax_freq],
    ['pwr:<=5', m.log_syntax_pwr],
    ['station:"JL1HIS/1"', m.log_syntax_quote],
    ['instrument:IC-705', m.log_syntax_instrument],
    ['park:US-0001', m.log_syntax_park],
    ['no:freq has:park', m.log_syntax_missing],
    ['adif.GRIDSQUARE:PM95', m.log_syntax_adif],
    ['dxcc:339,291', m.log_syntax_dxcc],
    ['yokohama', m.log_syntax_bare]
];
