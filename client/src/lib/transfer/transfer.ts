import type {
    Encoding,
    ExportKind,
    LengthUnit,
    MergeReport,
    NewReport,
    Outcome
} from '#lib/api/types.ts';
import { m } from '#lib/paraglide/messages.js';

export const MAX_IMPORT = 64 * 1024 * 1024;

export const KIND_LABELS: Record<ExportKind, () => string> = {
    adif: () => 'ADIF',
    pota: () => 'POTA',
    qsl_card: () => m.transfer_kind_qsl_card(),
    eqsl: () => 'eQSL',
    lotw: () => 'LoTW'
};

export const ENCODING_NAMES: Record<Encoding, string> = {
    'utf-8': 'UTF-8',
    shift_jis: 'Shift_JIS',
    'windows-1252': 'Windows-1252',
    ascii: 'ASCII'
};

export const LENGTH_LABELS: Record<LengthUnit, () => string> = {
    codepoints: () => m.transfer_length_codepoints(),
    bytes: () => m.transfer_length_bytes(),
    utf16: () => m.transfer_length_utf16()
};

export function readingNote(report: Pick<NewReport, 'encoding' | 'lengths'>): string {
    const encoding = m.transfer_read_encoding({ encoding: ENCODING_NAMES[report.encoding] });
    if (report.lengths.length === 0) return encoding;
    const units = report.lengths.map((unit) => LENGTH_LABELS[unit]()).join(', ');
    return `${encoding} · ${m.transfer_read_lengths({ units })}`;
}

export const SENT_FIELDS: Partial<Record<ExportKind, [string, string]>> = {
    qsl_card: ['QSL_SENT', 'QSLSDATE'],
    eqsl: ['EQSL_QSL_SENT', 'EQSL_QSLSDATE'],
    lotw: ['LOTW_QSL_SENT', 'LOTW_QSLSDATE']
};

export function recordOnly(kind: ExportKind): boolean {
    return SENT_FIELDS[kind] === undefined;
}

export function formatOf(name: string): 'adi' | 'adx' {
    return name.toLowerCase().endsWith('.adx') ? 'adx' : 'adi';
}

export function fileName(logbook: string, kind: ExportKind, at: Date, suffix = ''): string {
    const stamp = at.toISOString().slice(0, 16).replace(/[-:]/g, '').replace('T', '-');
    const safe = logbook.replace(/[^A-Za-z0-9_-]+/g, '_');
    return `${safe}-${kind.replace('_', '-')}${suffix}-${stamp}.adi`;
}

export function rejectCount(report: NewReport | MergeReport): number {
    return 'unmatched' in report
        ? report.invalid.length + report.unmatched.length + report.ambiguous.length
        : report.invalid.length;
}

export function differencesText(differences: Record<string, number>): string {
    return Object.entries(differences)
        .filter(([, n]) => n > 0)
        .sort(([, a], [, b]) => b - a)
        .map(([field, n]) => `${field} ${n}`)
        .join(', ');
}

export function outcomeText(outcome: Outcome): string {
    const updated = outcome.updated.length;
    const skipped = outcome.skipped.length;
    return skipped > 0
        ? m.transfer_outcome_updated_skipped({ updated, skipped })
        : m.transfer_outcome_updated({ updated });
}
