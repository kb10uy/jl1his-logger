import type { Area, Fields } from '#client/api/types.ts';
import { extraFieldProblem, type Entry } from '#client/entry/record.ts';
import { withBand, withFreq, withMode } from '#client/qso/tuning.ts';

export type EntryMessage = { type: 'set'; fields: Fields } | { type: 'clear' } | { type: 'log' };

const OWN = {
    CALL: 'call',
    RST_SENT: 'rstSent',
    RST_RCVD: 'rstRcvd',
    SRX_STRING: 'received',
    NAME: 'name',
    QTH: 'qth',
    COMMENT: 'comment',
    TX_PWR: 'txPwr',
    NOTES: 'notes'
} as const satisfies Record<string, keyof Entry>;

const TUNING = new Set(['FREQ', 'BAND', 'MODE', 'SUBMODE']);

const isOwn = (name: string): name is keyof typeof OWN => Object.hasOwn(OWN, name);

export function applyFields(entry: Entry, fields: Fields, area: Area): Entry {
    const given = new Map(
        Object.entries(fields).map(([name, value]) => [name.trim().toUpperCase(), value.trim()])
    );
    let next: Entry = { ...entry, extra: [...entry.extra] };
    const band = given.get('BAND');
    if (band) next = withBand(next, band);
    const freq = given.get('FREQ');
    if (freq) next = withFreq(next, freq);
    const mode = given.get('MODE');
    const submode = given.get('SUBMODE');
    if (mode) next = withMode(next, mode, submode ?? '');
    else if (submode !== undefined) next = withMode(next, next.mode, submode);
    for (const [name, value] of given) {
        if (TUNING.has(name)) continue;
        if (isOwn(name)) next[OWN[name]] = value;
        else if (area !== null && name === area.name) next.area = value;
        else if (extraFieldProblem(name, area) === undefined) {
            const rest = next.extra.filter((f) => f.name.trim().toUpperCase() !== name);
            next.extra = value === '' ? rest : [...rest, { name, value }];
        }
    }
    return next;
}
