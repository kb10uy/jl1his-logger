import type { Fields } from '#client/api/types.ts';
import { m } from '#lib/paraglide/messages.js';
import type { Mismatch } from './station.ts';

export interface Notification {
    title: string;
    body: string;
}

export function loggedNotification(
    program: string | null,
    fields: Fields,
    mismatches: Mismatch[]
): Notification {
    const source = program ? m.notify_from({ program }) : m.notify_from_unnamed();
    const summary = [source, fields.BAND, fields.SUBMODE || fields.MODE]
        .filter((s) => s)
        .join(' · ');
    return {
        title: m.notify_logged({ call: fields.CALL ?? '' }),
        body: [summary, ...mismatches.map((x) => m.entry_incoming_mismatch(x))].join('\n')
    };
}
