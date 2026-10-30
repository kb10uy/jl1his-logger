import * as api from '#lib/api/api.ts';
import { ApiError } from '#lib/api/client.ts';
import type { Logbook, Me } from '#lib/api/types.ts';
import { lastLogbook as lastLogbookSetting, load, save } from '#lib/device.ts';
import { forgetLogbooks } from '#lib/state/logbook.svelte.ts';

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
        forgetLogbooks();
    }

    expire() {
        this.me = null;
        forgetLogbooks();
    }

    logbook(id: string): Logbook | undefined {
        return this.logbooks.find((l) => l.id === id);
    }
}

export const session = new Session();

export const lastLogbook = () => load(lastLogbookSetting);

export const rememberLogbook = (id: string) => save(lastLogbookSetting, id);
