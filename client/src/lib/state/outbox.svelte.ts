import * as api from '#lib/api/api.ts';
import type { Fields, Saved } from '#lib/api/types.ts';
import { FIRST_TRY_MS, Outbox, type Outgoing, type Sent } from '#lib/entry/outbox.ts';
import { uuidv7 } from '#lib/qso/uuid.ts';

class OutboxState {
    items = $state.raw<Outgoing[]>([]);
    sending = $state(false);
    private outbox: Promise<Outbox> | undefined;
    private logbookId: string | undefined;
    private delivering = new Set<string>();

    private open(): Promise<Outbox> {
        this.outbox ??= Outbox.open();
        return this.outbox;
    }

    async show(logbookId: string) {
        this.logbookId = logbookId;
        await this.refresh();
    }

    private async refresh() {
        const logbookId = this.logbookId;
        if (logbookId === undefined) return;
        const items = await (await this.open()).list(logbookId);
        if (this.logbookId === logbookId)
            this.items = items.filter((i) => !this.delivering.has(i.id));
    }

    async log(
        logbookId: string,
        stationId: string,
        instrumentId: string | null,
        fields: Fields
    ): Promise<Sent> {
        const outbox = await this.open();
        const item: Outgoing = {
            id: uuidv7(),
            logbookId,
            stationId,
            instrumentId,
            fields,
            createdAt: new Date().toISOString()
        };
        this.delivering.add(item.id);
        try {
            await outbox.put(item);
            return await outbox.send(item, api.createQso, FIRST_TRY_MS);
        } finally {
            this.delivering.delete(item.id);
            await this.refresh();
        }
    }

    async send(items: Outgoing[]): Promise<{ saved: Saved[]; failed: number }> {
        const outbox = await this.open();
        const saved: Saved[] = [];
        let failed = 0;
        this.sending = true;
        try {
            for (const item of items) {
                const sent = await outbox.send(item, api.createQso);
                if ('saved' in sent) saved.push(sent.saved);
                else failed += 1;
            }
        } finally {
            this.sending = false;
            await this.refresh();
        }
        return { saved, failed };
    }

    async discard(id: string) {
        await (await this.open()).remove(id);
        await this.refresh();
    }
}

export const outbox = new OutboxState();
