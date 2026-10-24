import type { Fields } from '#lib/api/types.ts';

export type QslTone = 'ok' | 'warn' | 'none';

export type QslRequest = 'none' | 'requested' | 'oneWay';

export const QSL_REQUESTS: [QslRequest, string][] = [
    ['none', 'No QSL'],
    ['requested', 'QSL requested'],
    ['oneWay', 'QSL one way']
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

export const QSL_SENT_VALUES: [string, string][] = [
    ['N', 'No'],
    ['R', 'Requested'],
    ['Q', 'Queued'],
    ['Y', 'Yes'],
    ['I', 'Ignore']
];

export const QSL_RCVD_VALUES: [string, string][] = [
    ['N', 'No'],
    ['R', 'Requested'],
    ['Y', 'Yes'],
    ['V', 'Verified'],
    ['I', 'Ignore']
];

export const QSL_VIA_VALUES: [string, string][] = [
    ['B', 'Bureau'],
    ['D', 'Direct'],
    ['E', 'Electronic'],
    ['M', 'Manager']
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
    return table.find(([code]) => code === v)?.[1] ?? v;
}
