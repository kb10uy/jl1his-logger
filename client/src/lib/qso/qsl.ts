import type { Fields } from '#lib/api/types.ts';
import { m } from '#lib/paraglide/messages.js';

export type QslTone = 'ok' | 'warn' | 'none';

export type QslRequest = 'none' | 'requested' | 'oneWay';

export type QslChoice<T extends string = string> = [T, () => string];

export const QSL_REQUESTS: QslChoice<QslRequest>[] = [
    ['none', m.entry_qsl_request_none],
    ['requested', m.entry_qsl_request_requested],
    ['oneWay', m.entry_qsl_request_one_way]
];

export function qslRequestFields(request: QslRequest): Fields {
    switch (request) {
        case 'none':
            return {};
        case 'requested':
            return { QSL_SENT: 'R' };
        case 'oneWay':
            return { QSL_SENT: 'N', QSL_RCVD: 'R' };
    }
}

export const QSL_SENT_VALUES: QslChoice[] = [
    ['N', m.entry_qsl_no],
    ['R', m.entry_qsl_requested],
    ['Q', m.entry_qsl_queued],
    ['Y', m.entry_qsl_yes],
    ['I', m.entry_qsl_ignore]
];

export const QSL_RCVD_VALUES: QslChoice[] = [
    ['N', m.entry_qsl_no],
    ['R', m.entry_qsl_requested],
    ['Y', m.entry_qsl_yes],
    ['V', m.entry_qsl_verified],
    ['I', m.entry_qsl_ignore]
];

export const QSL_VIA_VALUES: QslChoice[] = [
    ['B', m.entry_qsl_via_bureau],
    ['D', m.entry_qsl_via_direct],
    ['E', m.entry_qsl_via_electronic],
    ['M', m.entry_qsl_via_manager]
];

export function qslValue(value: string | undefined): string {
    const v = (value ?? '').trim().toUpperCase();
    return v === '' ? 'N' : v;
}

export function qslTone(value: string | undefined): QslTone {
    const v = qslValue(value);
    if (v === 'Y' || v === 'V') return 'ok';
    if (v === 'R' || v === 'Q') return 'warn';
    return 'none';
}

export function qslLabel(value: string | undefined, sent: boolean): string {
    const v = qslValue(value);
    const table = sent ? QSL_SENT_VALUES : QSL_RCVD_VALUES;
    return table.find(([code]) => code === v)?.[1]() ?? v;
}
