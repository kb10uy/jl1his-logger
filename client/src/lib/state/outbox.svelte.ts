import * as api from '#lib/api/api.ts';
import type { Fields, Saved } from '#lib/api/types.ts';
import { Outbox, type Outgoing, type Sent } from '#lib/entry/outbox.ts';
import { uuidv7 } from '#lib/qso/uuid.ts';

class OutboxState {
    items = $state<Outgoing[]>([]);
    sending = $state(false);
    private outbox: Promise<Outbox> | undefined;
    private logbookId: string | undefined;

    private open(): Promise<Outbox> {
        this.outbox ??= Outbox.open();
        return this.outbox;
    }

    async show(logbookId: string) {
        this.logbookId = logbookId;
        const items = await (await this.open()).list(logbookId);
        if (this.logbookId === logbookId) this.items = items;
    }

    async log(logbookId: string, stationId: string, fields: Fields): Promise<Sent> {
        const outbox = await this.open();
        const item: Outgoing = {
            id: uuidv7(),
            logbookId,
            stationId,
            fields,
            createdAt: new Date().toISOString()
        };
        await outbox.put(item);
        await this.show(logbookId);
        const sent = await outbox.send(item, api.createQso);
        await this.show(logbookId);
        return sent;
    }

    async sendAll(): Promise<{ saved: Saved[]; failed: number }> {
        const outbox = await this.open();
        const saved: Saved[] = [];
        let failed = 0;
        this.sending = true;
        try {
            for (const item of this.items) {
                const sent = await outbox.send(item, api.createQso);
                if ('saved' in sent) saved.push(sent.saved);
                else failed += 1;
            }
        } finally {
            this.sending = false;
            if (this.logbookId !== undefined) await this.show(this.logbookId);
        }
        return { saved, failed };
    }

    async discard(id: string) {
        await (await this.open()).remove(id);
        if (this.logbookId !== undefined) await this.show(this.logbookId);
    }
}

export const outbox = new OutboxState();
