import * as api from '#lib/api/api.ts';
import type { ExportRequest } from '#lib/api/api.ts';
import type { Batch, Exported } from '#lib/api/types.ts';
import { download, fromBase64 } from '#lib/download.ts';
import { m } from '#lib/paraglide/messages.js';
import { toasts } from '#lib/state/toast.svelte.ts';
import { MAX_IMPORT, fileName, outcomeText } from '#lib/transfer/transfer.ts';

const message = (e: unknown) => (e instanceof Error ? e.message : String(e));

export class Batches {
    items = $state<Batch[]>([]);
    callsigns = $state<Record<string, string>>({});
    results = $state<Record<string, string>>({});
    applying = $state<string | undefined>(undefined);
    creating = $state(false);
    private readonly logbookId: string;
    private readonly logbookName: () => string;
    private readonly keep: (batch: Batch) => boolean;

    constructor(logbookId: string, logbookName: () => string, keep: (batch: Batch) => boolean) {
        this.logbookId = logbookId;
        this.logbookName = logbookName;
        this.keep = keep;
    }

    load() {
        api.batches(this.logbookId)
            .then((found) => (this.items = found.filter(this.keep)))
            .catch((e) => toasts.error(message(e)));
        api.members(this.logbookId)
            .then((found) => {
                this.callsigns = Object.fromEntries(found.map((m) => [m.user_id, m.callsign]));
            })
            .catch(() => undefined);
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
            this.items = [result.batch, ...this.items];
            return result;
        } catch (e) {
            toasts.error(message(e));
            return undefined;
        } finally {
            this.creating = false;
        }
    }

    async apply(batch: Batch): Promise<boolean> {
        this.applying = batch.id;
        try {
            const outcome = await api.applyBatch(this.logbookId, batch.id);
            this.results = { ...this.results, [batch.id]: outcomeText(outcome) };
            const appliedAt = new Date().toISOString();
            this.items = this.items.map((b) =>
                b.id === batch.id ? { ...b, applied_at: appliedAt } : b
            );
            return true;
        } catch (e) {
            toasts.error(message(e));
            return false;
        } finally {
            this.applying = undefined;
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
