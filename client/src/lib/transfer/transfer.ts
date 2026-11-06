import type {
    Batch,
    BatchPage,
    Encoding,
    ExportKind,
    ExportState,
    ExportedFile,
    LengthUnit,
    MergeReport,
    NewReport,
    Outcome
} from '#lib/api/types.ts';
import { m } from '#lib/paraglide/messages.js';
import { isAdifQsoField } from '#lib/qso/adif-fields.ts';

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

export type SplitChoice = 'none' | 'callsign' | 'callsign_location' | 'callsign_park' | 'custom';

export const SPLIT_FIELDS: Record<Exclude<SplitChoice, 'none' | 'custom'>, string[]> = {
    callsign: ['STATION_CALLSIGN'],
    callsign_location: ['STATION_CALLSIGN', 'MY_STATE', 'MY_CNTY', 'MY_GRIDSQUARE'],
    callsign_park: ['STATION_CALLSIGN', 'MY_SIG_INFO']
};

export function splitLabel(choice: SplitChoice): string {
    if (choice === 'none') return m.transfer_split_none();
    if (choice === 'custom') return m.transfer_split_custom();
    return SPLIT_FIELDS[choice].join(', ');
}

export function customSplitFields(text: string): { fields: string[]; unknown: string[] } {
    const names = [
        ...new Set(
            text
                .split(/[\s,]+/)
                .map((name) => name.toUpperCase())
                .filter((name) => name !== '')
        )
    ];
    return {
        fields: names.filter(isAdifQsoField),
        unknown: names.filter((name) => !isAdifQsoField(name))
    };
}

export function splitChoices(kind: ExportKind): SplitChoice[] {
    return kind === 'pota'
        ? ['none', 'callsign', 'callsign_park', 'custom']
        : ['none', 'callsign', 'callsign_location', 'custom'];
}

export function defaultSplit(kind: ExportKind): SplitChoice {
    switch (kind) {
        case 'lotw':
            return 'callsign_location';
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

export type ExportLists = Record<ExportState, BatchPage>;

export const NO_EXPORTS: ExportLists = {
    pending: { batches: [], next: null, total: 0 },
    finished: { batches: [], next: null, total: 0 }
};

export function exportCount(batches: Batch[]): number {
    return new Set(batches.map((b) => b.export_id)).size;
}

export function appendPage(list: BatchPage, page: BatchPage): BatchPage {
    return { ...page, batches: [...list.batches, ...page.batches] };
}

function withoutExport(list: BatchPage, exportId: string): BatchPage {
    const batches = list.batches.filter((b) => b.export_id !== exportId);
    return batches.length === list.batches.length
        ? list
        : { ...list, batches, total: list.total - 1 };
}

function withExport(list: BatchPage, batches: Batch[]): BatchPage {
    const id = batches[0].export_id;
    const total = list.total + 1;
    if (list.next !== null && id < list.next) return { ...list, total };
    const older = list.batches.findIndex((b) => b.export_id < id);
    const at = older < 0 ? list.batches.length : older;
    return {
        ...list,
        batches: [...list.batches.slice(0, at), ...batches, ...list.batches.slice(at)],
        total
    };
}

export function placeExport(lists: ExportLists, batches: Batch[]): ExportLists {
    const id = batches[0].export_id;
    const state = batches.some((b) => batchState(b) === 'pending') ? 'pending' : 'finished';
    const next = {
        pending: withoutExport(lists.pending, id),
        finished: withoutExport(lists.finished, id)
    };
    return { ...next, [state]: withExport(next[state], batches) };
}

export function reviseExport(
    lists: ExportLists,
    exportId: string,
    revise: (batch: Batch) => Batch
): ExportLists {
    const batches = [...lists.pending.batches, ...lists.finished.batches]
        .filter((b) => b.export_id === exportId)
        .map(revise);
    return batches.length === 0 ? lists : placeExport(lists, batches);
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
