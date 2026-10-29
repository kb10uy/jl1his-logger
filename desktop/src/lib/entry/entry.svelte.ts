import { untrack } from 'svelte';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import type {
    Fields,
    Hello,
    Instrument,
    Lookup,
    Qso,
    QsoEvent,
    Saved,
    Station
} from '#client/api/types.ts';
import type { EntrySettings } from '#client/entry/settings.ts';
import type { Sent } from '#client/entry/outbox.ts';
import { RECENT, mergeRecent } from '#client/entry/recent.ts';
import { usesSerial } from '#client/qso/contest.ts';
import { utcFields } from '#client/qso/format.ts';
import * as api from '#lib/api/api.ts';
import { server } from '#lib/api/request.svelte.ts';
import type { Logbook } from '#lib/session/types.ts';
import { chooseStation, stationGroups } from './choice.ts';
import { outbox } from './outbox.svelte.ts';
import { loadCache, loadSettings, saveCache, saveSettings } from './storage.ts';

export interface EntryInputs {
    readonly call: string | undefined;
    readonly freq: string;
    readonly band: string;
    readonly mode: string;
    readonly submode: string;
    readonly at: Date;
}

export type StreamEvent =
    | { type: 'hello'; logbookId: string; hello: Hello }
    | { type: 'qso'; logbookId: string; data: QsoEvent }
    | { type: 'status'; logbookId: string; live: boolean };

function later<T>(delay: number, load: () => Promise<T>, done: (value: T) => void): () => void {
    let live = true;
    const timer = setTimeout(() => {
        load()
            .then((value) => {
                if (live) done(value);
            })
            .catch(() => undefined);
    }, delay);
    return () => {
        live = false;
        clearTimeout(timer);
    };
}

export class EntryState {
    settings = $state<EntrySettings>(loadSettings());
    logbooks = $state.raw<Logbook[]>([]);
    stations = $state.raw<Record<string, Station[]>>({});
    instruments = $state.raw<Record<string, Instrument[]>>({});
    recent = $state.raw<Qso[]>([]);
    lookup = $state.raw<Lookup | undefined>(undefined);
    last = $state.raw<Qso | undefined>(undefined);
    duplicates = $state.raw<Qso[]>([]);
    logging = $state(false);
    loaded = $state(false);
    private resets = $state(0);
    private generations = new Map<string, string>();

    groups = $derived(stationGroups(this.logbooks, this.stations));
    choice = $derived(chooseStation(this.groups, this.settings.stationId));
    logbookId = $derived(this.choice?.logbookId ?? '');
    stationId = $derived(this.choice?.stationId ?? '');
    logbook = $derived(this.logbooks.find((l) => l.id === this.logbookId));
    activeInstruments = $derived(
        (this.instruments[this.logbookId] ?? []).filter((i) => i.archived_at === null)
    );
    instrumentId = $derived(
        this.activeInstruments.some((i) => i.id === this.settings.instrumentId)
            ? this.settings.instrumentId
            : ''
    );
    instrument = $derived(this.activeInstruments.find((i) => i.id === this.instrumentId));
    unsent = $derived(outbox.items.filter((i) => i.stationId === this.stationId));

    constructor(inputs: EntryInputs) {
        $effect(() => {
            saveSettings({
                stationId: this.settings.stationId,
                instrumentId: this.settings.instrumentId,
                freq: inputs.freq,
                band: inputs.band,
                mode: inputs.mode,
                submode: inputs.submode,
                contest: { ...this.settings.contest }
            });
        });

        $effect(() => {
            void outbox.show(this.logbooks.map((l) => l.id));
        });

        $effect(() => {
            const logbookIds = this.logbooks.map((l) => l.id);
            if (logbookIds.length === 0) return;
            invoke('watch', { logbookIds }).catch(() => undefined);
        });

        $effect(() => {
            const unlisten = listen<StreamEvent>('logbook-stream', (e) => this.stream(e.payload));
            return () => {
                void unlisten.then((stop) => stop());
            };
        });

        $effect(() => {
            const logbookId = this.logbookId;
            const stationId = this.stationId;
            void this.resets;
            this.recent = [];
            if (stationId === '') return;
            return later(
                0,
                () => api.searchQsos(logbookId, { q: `station-id:${stationId}`, limit: RECENT }),
                (page) => (this.recent = page.qsos)
            );
        });

        $effect(() => {
            const logbookId = this.logbookId;
            const call = inputs.call;
            this.lookup = undefined;
            this.last = undefined;
            if (call === undefined || logbookId === '') return;
            const lookup = later(
                250,
                () => api.lookup(logbookId, call),
                (found) => (this.lookup = found)
            );
            const last = later(
                250,
                () => api.searchQsos(logbookId, { q: `base:"${call}"`, limit: 1 }),
                (page) => (this.last = page.qsos[0])
            );
            return () => {
                lookup();
                last();
            };
        });

        $effect(() => {
            const logbookId = this.logbookId;
            const call = inputs.call;
            const band = inputs.band;
            const mode = inputs.mode;
            this.duplicates = [];
            if (call === undefined || logbookId === '') return;
            const at = utcFields(untrack(() => inputs.at));
            return later(
                300,
                () =>
                    api.duplicates(logbookId, {
                        call,
                        band,
                        mode,
                        qso_date: at.QSO_DATE,
                        time_on: at.TIME_ON
                    }),
                (found) => (this.duplicates = found.qsos)
            );
        });
    }

    private stream(event: StreamEvent) {
        switch (event.type) {
            case 'hello': {
                const before = this.generations.get(event.logbookId);
                this.generations.set(event.logbookId, event.hello.cursor_generation);
                if (before !== undefined && before !== event.hello.cursor_generation) {
                    if (event.logbookId === this.logbookId) this.resets += 1;
                }
                break;
            }
            case 'qso':
                if (event.logbookId === this.logbookId) {
                    this.recent = mergeRecent(this.recent, [event.data.qso], this.stationId);
                }
                break;
            case 'status':
                if (event.logbookId === this.logbookId) server.reachable = event.live;
                break;
        }
    }

    async load(logbooks: Logbook[] | null) {
        const cache = loadCache();
        const list = logbooks ?? cache.logbooks;
        const ids = new Set(list.map((l) => l.id));
        const kept = <T>(byLogbook: Record<string, T>) =>
            Object.fromEntries(Object.entries(byLogbook).filter(([id]) => ids.has(id)));
        this.logbooks = list;
        this.stations = kept(cache.stations);
        this.instruments = kept(cache.instruments);
        const results = await Promise.allSettled(
            list.map(async (l) => {
                const [stations, instruments] = await Promise.all([
                    api.stations(l.id),
                    api.instruments(l.id)
                ]);
                return { id: l.id, stations, instruments };
            })
        );
        const stations = { ...this.stations };
        const instruments = { ...this.instruments };
        for (const result of results) {
            if (result.status !== 'fulfilled') continue;
            stations[result.value.id] = result.value.stations;
            instruments[result.value.id] = result.value.instruments;
        }
        this.stations = stations;
        this.instruments = instruments;
        this.loaded = true;
        saveCache({ logbooks: list, stations, instruments });
    }

    async log(fields: Fields): Promise<Sent> {
        this.logging = true;
        try {
            const sent = await outbox.log(
                this.logbookId,
                this.stationId,
                this.instrumentId || null,
                fields
            );
            const contest = this.settings.contest;
            if (contest.enabled && usesSerial(contest.exchangeTemplate)) contest.nextSerial += 1;
            if ('saved' in sent) {
                this.recent = mergeRecent(this.recent, [sent.saved.qso], this.stationId);
            }
            return sent;
        } finally {
            this.logging = false;
        }
    }

    async sendUnsent(): Promise<{ saved: Saved[]; failed: number }> {
        const result = await outbox.send(this.unsent);
        this.recent = mergeRecent(
            this.recent,
            result.saved.map((s) => s.qso),
            this.stationId
        );
        return result;
    }
}
