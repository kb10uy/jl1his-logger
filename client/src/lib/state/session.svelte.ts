import * as api from '#lib/api/api.ts';
import { ApiError } from '#lib/api/client.ts';
import type { Logbook, Me } from '#lib/api/types.ts';

const LAST_LOGBOOK = 'jl1his-logger.logbook';

class Session {
    me = $state<Me | null | undefined>(undefined);
    logbooks = $state<Logbook[]>([]);
    error = $state<string | undefined>(undefined);

    async load() {
        try {
            const me = await api.me();
            this.logbooks = await api.logbooks();
            this.me = me;
            this.error = undefined;
        } catch (e) {
            if (e instanceof ApiError && e.status === 401) {
                this.me = null;
            } else {
                this.error = e instanceof Error ? e.message : String(e);
            }
        }
    }

    async signedIn(me: Me) {
        this.logbooks = await api.logbooks();
        this.me = me;
        this.error = undefined;
    }

    async signOut() {
        await api.signOut();
        this.me = null;
        this.logbooks = [];
    }

    expire() {
        this.me = null;
    }

    logbook(id: string): Logbook | undefined {
        return this.logbooks.find((l) => l.id === id);
    }
}

export const session = new Session();

export function lastLogbook(): string | null {
    try {
        return localStorage.getItem(LAST_LOGBOOK);
    } catch {
        return null;
    }
}

export function rememberLogbook(id: string) {
    try {
        localStorage.setItem(LAST_LOGBOOK, id);
    } catch {
        return;
    }
}
