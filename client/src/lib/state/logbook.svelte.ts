import { getContext, setContext } from 'svelte';
import * as api from '#lib/api/api.ts';
import type { RowMatcher } from '#lib/query/index.ts';
import type { Hello, Instrument, Logbook, Qso, QsoEvent, Station } from '#lib/api/types.ts';

export type StreamStatus = 'connecting' | 'live' | 'offline';

export class LogbookState {
    stations = $state<Station[]>([]);
    instruments = $state<Instrument[]>([]);
    status = $state<StreamStatus>('connecting');
    lastUpdatedSeq = $state(0);
    private generation: string | undefined;
    private source: EventSource | undefined;
    private listeners = new Set<(qso: Qso) => void>();
    private resetListeners = new Set<() => void>();

    constructor(readonly logbook: Logbook) {}

    get activeStations(): Station[] {
        return this.stations.filter((s) => s.archived_at === null);
    }

    stationName(id: string): string | undefined {
        return this.stations.find((s) => s.id === id)?.name;
    }

    station(id: string): Station | undefined {
        return this.stations.find((s) => s.id === id);
    }

    async loadStations() {
        this.stations = await api.stations(this.logbook.id);
    }

    get activeInstruments(): Instrument[] {
        return this.instruments.filter((i) => i.archived_at === null);
    }

    instrumentName(id: string | null): string | undefined {
        return this.instrument(id)?.name;
    }

    instrument(id: string | null): Instrument | undefined {
        return this.instruments.find((i) => i.id === id);
    }

    async loadInstruments() {
        this.instruments = await api.instruments(this.logbook.id);
    }

    matches(qso: Qso, query: RowMatcher | undefined): boolean | undefined {
        return query?.matches(
            qso.fields,
            this.stationName(qso.station_id),
            this.instrumentName(qso.instrument_id)
        );
    }

    start() {
        this.loadStations().catch(() => undefined);
        this.loadInstruments().catch(() => undefined);
        const source = new EventSource(api.eventsUrl(this.logbook.id));
        this.source = source;
        source.addEventListener('hello', (event) => {
            const hello = JSON.parse((event as MessageEvent<string>).data) as Hello;
            const changed =
                this.generation !== undefined && this.generation !== hello.cursor_generation;
            this.generation = hello.cursor_generation;
            this.lastUpdatedSeq = hello.last_updated_seq;
            this.status = 'live';
            if (changed) this.resetListeners.forEach((listener) => listener());
        });
        source.addEventListener('qso', (event) => {
            const message = event as MessageEvent<string>;
            const { qso } = JSON.parse(message.data) as QsoEvent;
            this.lastUpdatedSeq = Math.max(this.lastUpdatedSeq, qso.updated_seq);
            this.listeners.forEach((listener) => listener(qso));
        });
        source.addEventListener('open', () => {
            this.status = 'live';
        });
        source.addEventListener('error', () => {
            this.status = source.readyState === EventSource.CLOSED ? 'offline' : 'connecting';
        });
    }

    stop() {
        this.source?.close();
        this.source = undefined;
    }

    onQso(listener: (qso: Qso) => void): () => void {
        this.listeners.add(listener);
        return () => this.listeners.delete(listener);
    }

    onReset(listener: () => void): () => void {
        this.resetListeners.add(listener);
        return () => this.resetListeners.delete(listener);
    }
}

const KEY = Symbol('logbook');

export function provideLogbook(state: LogbookState) {
    setContext(KEY, state);
}

export function useLogbook(): LogbookState {
    return getContext<LogbookState>(KEY);
}
