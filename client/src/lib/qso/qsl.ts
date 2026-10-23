export type QslTone = 'ok' | 'warn' | 'none';

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
