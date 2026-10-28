import { openDB, type DBSchema, type IDBPDatabase } from 'idb';
import { ApiError } from '#lib/api/client.ts';
import type { Fields, Saved } from '#lib/api/types.ts';
import { m } from '#lib/paraglide/messages.js';

export interface Outgoing {
    id: string;
    logbookId: string;
    stationId: string;
    instrumentId?: string | null;
    fields: Fields;
    createdAt: string;
    error?: string;
}

interface OutboxDb extends DBSchema {
    outbox: {
        key: string;
        value: Outgoing;
        indexes: { logbook: string };
    };
}

export type Create = (
    logbookId: string,
    id: string,
    stationId: string,
    instrumentId: string | null,
    fields: Fields
) => Promise<Saved>;

export type Sent = { saved: Saved } | { error: ApiError };

export const FIRST_TRY_MS = 5000;

function within<T>(pending: Promise<T>, wait: number | undefined): Promise<T> {
    if (wait === undefined) return pending;
    return new Promise((resolve, reject) => {
        const timer = setTimeout(() => reject(new ApiError(m.error_unreachable(), 0)), wait);
        pending.then(resolve, reject).finally(() => clearTimeout(timer));
    });
}

export class Outbox {
    private constructor(private readonly db: IDBPDatabase<OutboxDb>) {}

    static async open(name = 'jl1his-logger'): Promise<Outbox> {
        const db = await openDB<OutboxDb>(name, 1, {
            upgrade(db) {
                db.createObjectStore('outbox', { keyPath: 'id' }).createIndex(
                    'logbook',
                    'logbookId'
                );
            }
        });
        return new Outbox(db);
    }

    async list(logbookId: string): Promise<Outgoing[]> {
        const items = await this.db.getAllFromIndex('outbox', 'logbook', logbookId);
        return items.sort((a, b) => a.createdAt.localeCompare(b.createdAt));
    }

    async put(item: Outgoing): Promise<void> {
        await this.db.put('outbox', structuredClone(item));
    }

    async remove(id: string): Promise<void> {
        await this.db.delete('outbox', id);
    }

    async send(item: Outgoing, create: Create, wait?: number): Promise<Sent> {
        try {
            const saved = await within(
                create(
                    item.logbookId,
                    item.id,
                    item.stationId,
                    item.instrumentId ?? null,
                    item.fields
                ),
                wait
            );
            await this.remove(item.id);
            return { saved };
        } catch (e) {
            const error = e instanceof ApiError ? e : new ApiError(String(e), 0);
            await this.put({ ...item, error: error.message });
            return { error };
        }
    }
}
