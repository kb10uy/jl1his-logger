import type { Fields, Saved } from '#client/api/types.ts';
import { FIRST_TRY_MS, Outbox, type Outgoing, type Sent } from '#client/entry/outbox.ts';
import { uuidv7 } from '#client/qso/uuid.ts';
import * as api from '#lib/api/api.ts';

class OutboxState {
    items = $state.raw<Outgoing[]>([]);
    sending = $state(false);
    private outbox: Promise<Outbox> | undefined;
    private logbookIds: string[] = [];
    private delivering = new Set<string>();

    private open(): Promise<Outbox> {
        this.outbox ??= Outbox.open();
        return this.outbox;
    }

    async show(logbookIds: string[]) {
        this.logbookIds = logbookIds;
        await this.refresh();
    }

    private async refresh() {
        const logbookIds = this.logbookIds;
        const outbox = await this.open();
        const lists = await Promise.all(logbookIds.map((id) => outbox.list(id)));
        if (this.logbookIds === logbookIds)
            this.items = lists.flat().filter((i) => !this.delivering.has(i.id));
    }

    async log(
        logbookId: string,
        stationId: string,
        instrumentId: string | null,
        fields: Fields
    ): Promise<Sent> {
        return this.deliver(await this.queue(logbookId, stationId, instrumentId, fields));
    }

    async queue(
        logbookId: string,
        stationId: string,
        instrumentId: string | null,
        fields: Fields
    ): Promise<Outgoing> {
        const outbox = await this.open();
        const item: Outgoing = {
            id: uuidv7(),
            logbookId,
            stationId,
            instrumentId,
            fields,
            createdAt: new Date().toISOString()
        };
        await outbox.put(item);
        return item;
    }

    async deliver(item: Outgoing): Promise<Sent> {
        this.delivering.add(item.id);
        try {
            return await (await this.open()).send(item, api.createQso, FIRST_TRY_MS);
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
