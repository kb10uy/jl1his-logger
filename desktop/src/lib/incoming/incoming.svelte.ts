import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import type { Fields } from '#client/api/types.ts';
import { chooseStation, stationGroups } from '#lib/entry/choice.ts';
import { outbox } from '#lib/entry/outbox.svelte.ts';
import { loadCache, loadSettings } from '#lib/entry/storage.ts';
import { stationMismatches, type Mismatch } from './station.ts';

interface Incoming {
    id: number;
    program: string | null;
    fields: Fields;
}

export type Notice =
    | { kind: 'logged'; program: string | null; call: string; mismatches: Mismatch[] }
    | { kind: 'waiting'; count: number };

export const warns = (notice: Notice) => notice.kind === 'waiting' || notice.mismatches.length > 0;

class IncomingState {
    notice = $state.raw<Notice | undefined>(undefined);
    private started = false;
    private running = false;
    private again = false;

    start() {
        if (this.started) return;
        this.started = true;
        void listen('incoming-qso', () => this.drain());
        void this.drain();
    }

    async drain() {
        if (this.running) {
            this.again = true;
            return;
        }
        this.running = true;
        try {
            do {
                this.again = false;
                await this.take();
            } while (this.again);
        } catch {
            return;
        } finally {
            this.running = false;
        }
    }

    private async take() {
        const received = await invoke<Incoming[]>('incoming');
        if (received.length === 0) return;
        const settings = loadSettings();
        const cache = loadCache();
        const choice = chooseStation(
            stationGroups(cache.logbooks, cache.stations),
            settings.stationId
        );
        const station =
            choice && cache.stations[choice.logbookId]?.find((s) => s.id === choice.stationId);
        if (choice === undefined || station === undefined) {
            this.notice = { kind: 'waiting', count: received.length };
            return;
        }
        const instrumentId = (cache.instruments[choice.logbookId] ?? []).some(
            (i) => i.id === settings.instrumentId && i.archived_at === null
        )
            ? settings.instrumentId
            : null;
        for (const { id, program, fields } of received) {
            const item = await outbox.queue(
                choice.logbookId,
                choice.stationId,
                instrumentId,
                fields
            );
            await invoke('ack_incoming', { ids: [id] });
            const notice: Notice = {
                kind: 'logged',
                program,
                call: fields.CALL ?? '',
                mismatches: stationMismatches(fields, station)
            };
            const shown = this.notice;
            if (shown === undefined || shown.kind === 'waiting' || !warns(shown) || warns(notice))
                this.notice = notice;
            await outbox.deliver(item);
        }
    }
}

export const incoming = new IncomingState();
