import type { Fields, Saved } from '#client/api/types.ts';
import { Outbox, type Outgoing, type Sent } from '#client/entry/outbox.ts';
import { uuidv7 } from '#client/qso/uuid.ts';
import * as api from '#lib/api/api.ts';

class OutboxState {
    items = $state.raw<Outgoing[]>([]);
    sending = $state(false);
    private outbox: Promise<Outbox> | undefined;
    private logbookIds: string[] = [];

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
        if (this.logbookIds === logbookIds) this.items = lists.flat();
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
        await outbox.put(item);
        await this.refresh();
        const sent = await outbox.send(item, api.createQso);
        await this.refresh();
        return sent;
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
