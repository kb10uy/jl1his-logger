import { getContext, setContext } from 'svelte';
import * as api from '#lib/api/api.ts';
import type { RowMatcher } from '#lib/query/index.ts';
import type { Area, Hello, Instrument, Logbook, Qso, QsoEvent, Station } from '#lib/api/types.ts';

export type StreamStatus = 'connecting' | 'live' | 'offline';

const RETRY_MIN = 2000;
const RETRY_MAX = 60000;

export class LogbookState {
    logbook = $state<Logbook>() as Logbook;
    stations = $state<Station[]>([]);
    instruments = $state<Instrument[]>([]);
    loaded = $state(false);
    loadError = $state<string | undefined>(undefined);
    status = $state<StreamStatus>('connecting');
    lastUpdatedSeq = $state(0);
    private generation: string | undefined;
    private source: EventSource | undefined;
    private retry: ReturnType<typeof setTimeout> | undefined;
    private delay = RETRY_MIN;
    private listeners = new Set<(qso: Qso) => void>();
    private resetListeners = new Set<() => void>();

    constructor(logbook: Logbook) {
        this.logbook = logbook;
    }

    get area(): Area {
        return this.logbook.area_field;
    }

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

    activeInstrumentId(id: string): string {
        return this.activeInstruments.some((i) => i.id === id) ? id : '';
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

    async load() {
        const results = await Promise.allSettled([this.loadStations(), this.loadInstruments()]);
        const failed = results.find((r): r is PromiseRejectedResult => r.status === 'rejected');
        this.loadError =
            failed === undefined
                ? undefined
                : failed.reason instanceof Error
                  ? failed.reason.message
                  : String(failed.reason);
        this.loaded = true;
    }

    get noActiveStation(): boolean {
        return this.loaded && this.loadError === undefined && this.activeStations.length === 0;
    }

    matches(qso: Qso, query: RowMatcher | undefined): boolean | undefined {
        return query?.matches({
            fields: qso.fields,
            stationId: qso.station_id,
            stationName: this.stationName(qso.station_id),
            instrumentId: qso.instrument_id,
            instrumentName: this.instrumentName(qso.instrument_id)
        });
    }

    start() {
        void this.load();
        this.connect(undefined);
    }

    private connect(after: number | undefined) {
        const source = new EventSource(api.eventsUrl(this.logbook.id, after));
        this.source = source;
        source.addEventListener('hello', (event) => {
            const hello = JSON.parse((event as MessageEvent<string>).data) as Hello;
            const changed =
                this.generation !== undefined && this.generation !== hello.cursor_generation;
            this.generation = hello.cursor_generation;
            this.lastUpdatedSeq = hello.last_updated_seq;
            this.status = 'live';
            this.delay = RETRY_MIN;
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
            if (source.readyState !== EventSource.CLOSED) {
                this.status = 'connecting';
                return;
            }
            this.status = 'offline';
            this.reconnect();
        });
    }

    private reconnect() {
        clearTimeout(this.retry);
        const after = this.generation === undefined ? undefined : this.lastUpdatedSeq;
        this.retry = setTimeout(() => this.connect(after), this.delay);
        this.delay = Math.min(this.delay * 2, RETRY_MAX);
    }

    stop() {
        clearTimeout(this.retry);
        this.retry = undefined;
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
