import { untrack } from 'svelte';
import * as api from '#lib/api/api.ts';
import type { Fields, Lookup, Qso, Saved } from '#lib/api/types.ts';
import { LOOKUP_DELAY, candidateOf } from '#lib/entry/lookup.ts';
import type { Sent } from '#lib/entry/outbox.ts';
import { RECENT, mergeRecent } from '#lib/entry/recent.ts';
import { loadEntry, saveEntry, type EntrySettings } from '#lib/entry/settings.ts';
import { usesSerial } from '#lib/qso/contest.ts';
import type { LogbookState } from '#lib/state/logbook.svelte.ts';
import { outbox } from '#lib/state/outbox.svelte.ts';

export interface EntryInputs {
    readonly call: string | undefined;
    readonly freq: string;
    readonly band: string;
    readonly mode: string;
    readonly submode: string;
    readonly at: Date;
}

export class EntrySession {
    settings = $state<EntrySettings>() as EntrySettings;
    recent = $state<Qso[]>([]);
    lookup = $state<Lookup | undefined>(undefined);
    duplicates = $state<Qso[]>([]);
    logging = $state(false);
    readonly logbook: LogbookState;
    readonly logbookId: string;

    stationId = $derived.by(() =>
        this.logbook.activeStations.some((s) => s.id === this.settings.stationId)
            ? this.settings.stationId
            : (this.logbook.activeStations[0]?.id ?? '')
    );
    instrumentId = $derived.by(() => this.logbook.activeInstrumentId(this.settings.instrumentId));
    unsent = $derived.by(() => outbox.items.filter((i) => i.stationId === this.stationId));

    constructor(logbook: LogbookState, inputs: EntryInputs) {
        this.logbook = logbook;
        this.logbookId = logbook.logbook.id;
        this.settings = loadEntry(this.logbookId);

        $effect(() =>
            logbook.onQso((qso) => {
                this.recent = mergeRecent(this.recent, [qso], this.stationId);
            })
        );

        $effect(() => {
            saveEntry(this.logbookId, {
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
            const station = this.stationId;
            this.recent = [];
            if (station === '') return;
            const controller = new AbortController();
            api.searchQsos(
                this.logbookId,
                { q: `station-id:${station}`, limit: RECENT },
                controller.signal
            )
                .then((page) => (this.recent = page.qsos))
                .catch(() => undefined);
            return () => controller.abort();
        });

        $effect(() => {
            const call = inputs.call;
            const band = inputs.band;
            const mode = inputs.mode;
            this.lookup = undefined;
            this.duplicates = [];
            if (call === undefined) return;
            const controller = new AbortController();
            const timer = setTimeout(() => {
                const candidate = candidateOf(
                    band,
                    mode,
                    untrack(() => inputs.at)
                );
                api.lookup(this.logbookId, call, candidate, controller.signal)
                    .then((found) => {
                        this.lookup = found;
                        this.duplicates = found.duplicates ?? [];
                    })
                    .catch(() => undefined);
            }, LOOKUP_DELAY);
            return () => {
                clearTimeout(timer);
                controller.abort();
            };
        });
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
