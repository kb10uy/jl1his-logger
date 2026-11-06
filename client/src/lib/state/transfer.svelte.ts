import * as api from '#lib/api/api.ts';
import type { ExportRequest } from '#lib/api/api.ts';
import type {
    Batch,
    Encoding,
    ExportKind,
    ExportState,
    Exported,
    ImportSource,
    LengthUnit,
    MergeReport,
    NewReport,
    SplitExported,
    Unencodable
} from '#lib/api/types.ts';
import { download, fromBase64 } from '#lib/download.ts';
import { m } from '#lib/paraglide/messages.js';
import type { LogbookState } from '#lib/state/logbook.svelte.ts';
import { toasts } from '#lib/state/toast.svelte.ts';
import {
    type ExportLists,
    MAX_IMPORT,
    NO_EXPORTS,
    type SplitChoice,
    appendPage,
    batchState,
    exportCount,
    fileName,
    formatOf,
    outcomeText,
    placeExport,
    reviseExport
} from '#lib/transfer/transfer.ts';

const EXPORTS_SHOWN = 10;
const EXPORTS_MORE = 50;
const EXPORT_STATES: ExportState[] = ['pending', 'finished'];

const message = (e: unknown) => (e instanceof Error ? e.message : String(e));

export class Batches {
    lists = $state.raw<ExportLists>(NO_EXPORTS);
    callsigns = $state<Record<string, string>>({});
    results = $state<Record<string, string>>({});
    working = $state<string | undefined>(undefined);
    creating = $state(false);
    private readonly logbookId: string;
    private readonly logbookName: () => string;

    constructor(logbookId: string, logbookName: () => string) {
        this.logbookId = logbookId;
        this.logbookName = logbookName;
    }

    async load(): Promise<void> {
        api.members(this.logbookId)
            .then((found) => {
                this.callsigns = Object.fromEntries(found.map((m) => [m.user_id, m.callsign]));
            })
            .catch(() => undefined);
        await this.work('load', async () => {
            const limit = EXPORTS_SHOWN;
            const [pending, finished] = await Promise.all(
                EXPORT_STATES.map((state) => api.batches(this.logbookId, { state, limit }))
            );
            this.lists = { pending, finished };
        });
    }

    async more(state: ExportState): Promise<void> {
        await this.work(state, () => this.extend(state, EXPORTS_MORE));
    }

    async create(req: ExportRequest): Promise<Exported | undefined> {
        this.creating = true;
        try {
            const result = await api.createExport(this.logbookId, req);
            if (result.batch === null || result.adif === null) return result;
            download(
                fileName(this.logbookName(), req.kind, new Date()),
                result.file === null ? result.adif : fromBase64(result.file)
            );
            this.lists = placeExport(this.lists, [result.batch]);
            return result;
        } catch (e) {
            toasts.error(message(e));
            return undefined;
        } finally {
            this.creating = false;
        }
    }

    async createSplit(req: ExportRequest, split: string[]): Promise<SplitExported | undefined> {
        this.creating = true;
        try {
            const result = await api.createSplitExport(this.logbookId, { ...req, split });
            if (result.format === null || result.file === null) return result;
            const suffix = result.format === 'adi' ? `-${result.files[0].name.slice(0, -4)}` : '';
            download(
                fileName(this.logbookName(), req.kind, new Date(), suffix, result.format),
                fromBase64(result.file)
            );
            this.lists = placeExport(
                this.lists,
                result.files.map((f) => f.batch)
            );
            return result;
        } catch (e) {
            toasts.error(message(e));
            return undefined;
        } finally {
            this.creating = false;
        }
    }

    async apply(batch: Batch): Promise<boolean> {
        return this.work(batch.id, async () => {
            const outcome = await api.applyBatch(this.logbookId, batch.id);
            const result = outcomeText(outcome);
            this.results = { ...this.results, [batch.id]: result };
            toasts.show(m.transfer_batch_applied_result({ result }));
            this.replace({ ...batch, applied_at: new Date().toISOString() });
        });
    }

    async dismiss(batch: Batch): Promise<boolean> {
        return this.work(batch.id, async () => {
            this.replace(await api.dismissBatch(this.logbookId, batch.id));
        });
    }

    async restore(batch: Batch): Promise<boolean> {
        return this.work(batch.id, async () => {
            this.replace(await api.restoreBatch(this.logbookId, batch.id));
        });
    }

    async applyExport(exportId: string): Promise<boolean> {
        return this.work(exportId, async () => {
            const outcome = await api.applyExport(this.logbookId, exportId);
            this.settle(exportId, 'applied_at');
            toasts.show(
                m.transfer_export_applied({
                    batches: outcome.batches,
                    result: outcomeText(outcome)
                })
            );
        });
    }

    async dismissExport(exportId: string): Promise<boolean> {
        return this.work(exportId, async () => {
            const { batches } = await api.dismissExport(this.logbookId, exportId);
            this.settle(exportId, 'dismissed_at');
            toasts.show(m.transfer_export_dismissed({ batches }));
        });
    }

    private replace(batch: Batch) {
        this.lists = reviseExport(this.lists, batch.export_id, (b) =>
            b.id === batch.id ? batch : b
        );
    }

    private settle(exportId: string, field: 'applied_at' | 'dismissed_at') {
        const at = new Date().toISOString();
        this.lists = reviseExport(this.lists, exportId, (b) =>
            batchState(b) === 'pending' ? { ...b, [field]: at } : b
        );
    }

    private async extend(state: ExportState, limit: number): Promise<void> {
        const after = this.lists[state].next;
        if (after === null) return;
        const page = await api.batches(this.logbookId, { state, limit, after });
        this.lists = { ...this.lists, [state]: appendPage(this.lists[state], page) };
    }

    private async refill(): Promise<void> {
        try {
            for (const state of EXPORT_STATES) {
                const missing = EXPORTS_SHOWN - exportCount(this.lists[state].batches);
                if (missing > 0) await this.extend(state, missing);
            }
        } catch (e) {
            toasts.error(message(e));
        }
    }

    private async work(id: string, run: () => Promise<void>): Promise<boolean> {
        this.working = id;
        try {
            await run();
            await this.refill();
            return true;
        } catch (e) {
            toasts.error(message(e));
            return false;
        } finally {
            this.working = undefined;
        }
    }
}

export class ImportRun<R> {
    report = $state<R | undefined>(undefined);
    applied = $state<R | undefined>(undefined);
    error = $state<string | undefined>(undefined);
    previewing = $state(false);
    applying = $state(false);
    private attempt = 0;
    private readonly send: (file: File, apply: boolean) => Promise<R>;

    constructor(send: (file: File, apply: boolean) => Promise<R>) {
        this.send = send;
    }

    preview(file: File) {
        this.applied = undefined;
        return this.run(file, false);
    }

    async run(file: File, apply: boolean): Promise<R | undefined> {
        const mine = ++this.attempt;
        this.previewing = false;
        if (file.size > MAX_IMPORT) {
            this.report = undefined;
            this.error = m.transfer_file_too_large();
            return undefined;
        }
        this.error = undefined;
        if (apply) this.applying = true;
        else this.previewing = true;
        try {
            const result = await this.send(file, apply);
            if (mine === this.attempt) {
                this.report = result;
                this.applied = apply ? result : undefined;
            }
            return result;
        } catch (e) {
            if (mine === this.attempt) {
                this.report = undefined;
                this.error = message(e);
            } else if (apply) {
                toasts.error(message(e));
            }
            return undefined;
        } finally {
            if (apply) this.applying = false;
            if (mine === this.attempt) this.previewing = false;
        }
    }
}

export class ExportDraft {
    kind = $state<ExportKind>('adif');
    q = $state('');
    park = $state('');
    length = $state<LengthUnit>('codepoints');
    encoding = $state<Encoding>('utf-8');
    unencodable = $state<Unencodable>('error');
    split = $state<SplitChoice>('none');
    customSplit = $state('');
}

export class ImportDraft {
    source = $state<ImportSource>('new');
    file = $state<File | undefined>(undefined);
    stationId = $state('');
    instrumentId = $state('');
    length = $state<LengthUnit | 'auto'>('auto');
    encoding = $state<Encoding | 'auto'>('auto');
}

export class TransferState {
    readonly batches: Batches;
    readonly exportDraft = new ExportDraft();
    readonly importDraft = new ImportDraft();
    readonly importer: ImportRun<NewReport | MergeReport>;

    constructor(logbook: LogbookState) {
        const id = logbook.logbook.id;
        this.batches = new Batches(id, () => logbook.logbook.name);
        this.importer = new ImportRun<NewReport | MergeReport>((file, apply) => {
            const draft = this.importDraft;
            const format = formatOf(file.name);
            const options = {
                format,
                length: format === 'adi' && draft.length !== 'auto' ? draft.length : undefined,
                encoding:
                    format === 'adi' && draft.encoding !== 'auto' ? draft.encoding : undefined,
                apply
            };
            return draft.source === 'new'
                ? api.importAdif(
                      id,
                      draft.source,
                      {
                          ...options,
                          station_id: this.station(logbook),
                          instrument_id: logbook.activeInstrumentId(draft.instrumentId) || undefined
                      },
                      file
                  )
                : api.importAdif(
                      id,
                      draft.source,
                      { ...options, station_id: draft.stationId || undefined },
                      file
                  );
        });
    }

    station(logbook: LogbookState): string {
        const id = this.importDraft.stationId;
        return logbook.activeStations.some((s) => s.id === id)
            ? id
            : (logbook.activeStations[0]?.id ?? '');
    }
}

const states = new Map<string, TransferState>();

export function transferState(logbook: LogbookState): TransferState {
    let state = states.get(logbook.logbook.id);
    if (state === undefined) {
        state = new TransferState(logbook);
        states.set(logbook.logbook.id, state);
    }
    return state;
}

export function forgetTransfers() {
    states.clear();
}
