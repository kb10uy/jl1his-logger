import { m } from '#lib/paraglide/messages.js';
import { bandNamed, formatMhz, frequencyHz } from '#lib/qso/bands.ts';
import { defaultRst } from '#lib/qso/modes.ts';
import * as tuning from '#lib/qso/tuning.ts';
import { formatFrequencyPattern, resolveFrequency } from '#lib/voice/dsl/frequency.ts';
import type { QslStatus } from '#lib/voice/dsl/lexicon.ts';
import { modeLabel } from '#lib/voice/dsl/mode.ts';
import type { SpokenUpdate } from '#lib/voice/dsl/parser.ts';

export interface Carry {
    freq: string;
    band: string;
    mode: string;
    submode: string;
}

export interface Draft extends Carry {
    call?: string;
    rstSent: string;
    rstRcvd: string;
    area?: string;
    qsl: QslStatus;
    received?: string;
    name?: string;
    qth?: string;
    startedAt?: string;
}

export type FreeTextField = 'name' | 'qth';

export const FREE_TEXT_MAX_LENGTH = 100;
const FALLBACK_ANCHOR_HZ = 433_000_000;

export function newDraft(carry: Carry): Draft {
    const rst = defaultRst(carry.mode, carry.submode);
    return { ...carry, rstSent: rst, rstRcvd: rst, qsl: 'none' };
}

export function carryOf(draft: Draft): Carry {
    return { freq: draft.freq, band: draft.band, mode: draft.mode, submode: draft.submode };
}

export function isPristine(draft: Draft): boolean {
    return draft.startedAt === undefined;
}

export function anchorHz(carry: Carry): number {
    const current = frequencyHz(carry.freq);
    if (current !== undefined && current > 0) return current;
    const band = bandNamed(carry.band);
    return band === undefined ? FALLBACK_ANCHOR_HZ : Math.round((band.lowerHz + band.upperHz) / 2);
}

export function withMode(draft: Draft, mode: string, submode: string): Draft {
    return tuning.withMode(draft, mode, submode);
}

export function withBand(draft: Draft, band: string): Draft {
    return tuning.withBand(draft, band);
}

export function withFreq(draft: Draft, freq: string): Draft {
    return tuning.withFreq(draft, freq);
}

export function withFrequency(draft: Draft, hz: number): Draft {
    return withFreq(draft, formatMhz(hz));
}

export function setFreeText(
    draft: Draft,
    field: FreeTextField,
    value: string,
    now: Date = new Date()
): Draft {
    const text = value.replace(/\s+/g, ' ').trim();
    const next: Draft = { ...draft, [field]: text === '' ? undefined : text };
    if (text !== '' && next.startedAt === undefined) next.startedAt = now.toISOString();
    return next;
}

export function modeText(mode: string, submode: string): string {
    const { label, detail } = modeLabel({ mode, submode: submode === '' ? undefined : submode });
    return detail === undefined ? label : `${label} (${detail})`;
}

export function describeUpdate(update: SpokenUpdate, after: Draft): string {
    switch (update.kind) {
        case 'callsign':
            return m.voice_change_call({ value: update.value });
        case 'rstSent':
            return m.voice_change_rst_sent({ value: update.value });
        case 'rstReceived':
            return m.voice_change_rst_received({ value: update.value });
        case 'frequency':
            return m.voice_change_frequency({
                pattern: formatFrequencyPattern(update.value),
                freq: after.freq
            });
        case 'area':
            return m.voice_change_area({ value: update.value });
        case 'qsl':
            return {
                none: m.voice_qsl_none,
                requested: m.voice_qsl_requested,
                oneWay: m.voice_qsl_one_way
            }[update.value]();
        case 'mode':
            return m.voice_change_mode({ mode: modeText(after.mode, after.submode) });
        case 'exchangeReceived':
            return m.voice_change_number({ value: update.value });
    }
}

export function applyUpdate(draft: Draft, update: SpokenUpdate, now: Date = new Date()): Draft {
    const next: Draft = { ...draft, startedAt: draft.startedAt ?? now.toISOString() };
    switch (update.kind) {
        case 'callsign':
            return { ...next, call: update.value };
        case 'rstSent':
            return { ...next, rstSent: update.value };
        case 'rstReceived':
            return { ...next, rstRcvd: update.value };
        case 'frequency':
            return withFrequency(
                next,
                resolveFrequency(update.value, { anchorHz: anchorHz(next) })
            );
        case 'area':
            return { ...next, area: update.value };
        case 'qsl':
            return { ...next, qsl: update.value };
        case 'mode':
            return withMode(next, update.value.mode, update.value.submode ?? '');
        case 'exchangeReceived':
            return { ...next, received: update.value };
    }
}

export function applyUpdates(
    draft: Draft,
    updates: readonly SpokenUpdate[],
    now: Date = new Date()
): { draft: Draft; descriptions: string[] } {
    let current = draft;
    const descriptions: string[] = [];
    for (const update of updates) {
        current = applyUpdate(current, update, now);
        descriptions.push(describeUpdate(update, current));
    }
    return { draft: current, descriptions };
}
