import type {
    Batch,
    Encoding,
    ExportKind,
    ExportedFile,
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

export const QUERY_PRESETS: Record<ExportKind, string> = {
    adif: '',
    pota: '',
    qsl_card: 'sent:R,Q',
    eqsl: '-eqsl-sent:Y',
    lotw: '-lotw-sent:Y'
};

export function formatOf(name: string): 'adi' | 'adx' {
    return name.toLowerCase().endsWith('.adx') ? 'adx' : 'adi';
}

export function fileName(
    logbook: string,
    kind: ExportKind,
    at: Date,
    suffix = '',
    extension = 'adi'
): string {
    const stamp = at.toISOString().slice(0, 16).replace(/[-:]/g, '').replace('T', '-');
    const safe = logbook.replace(/[^A-Za-z0-9_-]+/g, '_');
    return `${safe}-${kind.replace('_', '-')}${suffix}-${stamp}.${extension}`;
}

export type SplitChoice = 'none' | 'callsign' | 'callsign_grid' | 'callsign_park';

export const SPLIT_FIELDS: Record<Exclude<SplitChoice, 'none'>, string[]> = {
    callsign: ['STATION_CALLSIGN'],
    callsign_grid: ['STATION_CALLSIGN', 'MY_GRIDSQUARE'],
    callsign_park: ['STATION_CALLSIGN', 'MY_SIG_INFO']
};

export function splitLabel(choice: SplitChoice): string {
    return choice === 'none' ? m.transfer_split_none() : SPLIT_FIELDS[choice].join(', ');
}

export function splitChoices(kind: ExportKind): SplitChoice[] {
    return kind === 'pota'
        ? ['none', 'callsign', 'callsign_park']
        : ['none', 'callsign', 'callsign_grid'];
}

export function defaultSplit(kind: ExportKind): SplitChoice {
    switch (kind) {
        case 'lotw':
            return 'callsign_grid';
        case 'eqsl':
            return 'callsign';
        case 'pota':
            return 'callsign_park';
        default:
            return 'none';
    }
}

export function missingCount(files: ExportedFile[]): number {
    return files.filter((f) => f.missing.length > 0).reduce((n, f) => n + f.records, 0);
}

export type BatchState = 'record' | 'pending' | 'applied' | 'dismissed';

export function batchState(batch: Batch): BatchState {
    if (recordOnly(batch.kind)) return 'record';
    if (batch.applied_at !== null) return 'applied';
    if (batch.dismissed_at !== null) return 'dismissed';
    return 'pending';
}

export interface ExportRun {
    id: string;
    kind: ExportKind;
    created_by: string;
    created_at: string;
    batches: Batch[];
    qsos: number;
    pending: number;
    applied: number;
    dismissed: number;
}

export function exportRuns(batches: Batch[]): ExportRun[] {
    const runs = new Map<string, ExportRun>();
    for (const batch of batches) {
        let run = runs.get(batch.export_id);
        if (run === undefined) {
            run = {
                id: batch.export_id,
                kind: batch.kind,
                created_by: batch.created_by,
                created_at: batch.created_at,
                batches: [],
                qsos: 0,
                pending: 0,
                applied: 0,
                dismissed: 0
            };
            runs.set(batch.export_id, run);
        }
        run.batches.push(batch);
        run.qsos += batch.qso_count;
        if (batch.created_at < run.created_at) run.created_at = batch.created_at;
        const state = batchState(batch);
        if (state !== 'record') run[state] += 1;
    }
    for (const run of runs.values()) {
        run.batches.sort((a, b) => (a.label ?? '').localeCompare(b.label ?? ''));
    }
    return [...runs.values()];
}

export function partitionRuns(runs: ExportRun[]): { pending: ExportRun[]; finished: ExportRun[] } {
    return {
        pending: runs.filter((run) => run.pending > 0),
        finished: runs.filter((run) => run.pending === 0)
    };
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
