import type { Fields } from '#lib/api/types.ts';
import { ADIF_QSO_FIELDS, isAdifQsoField } from '#lib/qso/adif-fields.ts';
import {
    contestProblem,
    exchangeString,
    expandExchange,
    type ContestSettings
} from '#lib/qso/contest.ts';
import { utcFields } from '#lib/qso/format.ts';
import { isPower, POWER_RULE } from '#lib/qso/power.ts';
import { qslRequestFields, type QslRequest } from '#lib/qso/qsl.ts';

export interface ExtraField {
    name: string;
    value: string;
}

export interface Entry {
    call: string;
    freq: string;
    band: string;
    mode: string;
    submode: string;
    rstSent: string;
    rstRcvd: string;
    received: string;
    name: string;
    qth: string;
    cnty: string;
    qsl: QslRequest;
    comment: string;
    txPwr: string;
    notes: string;
    extra: ExtraField[];
}

const ENTERED = new Set([
    'CALL',
    'QSO_DATE',
    'TIME_ON',
    'FREQ',
    'BAND',
    'MODE',
    'SUBMODE',
    'RST_SENT',
    'RST_RCVD',
    'NAME',
    'QTH',
    'CNTY',
    'QSL_SENT',
    'QSL_RCVD',
    'COMMENT',
    'TX_PWR',
    'NOTES',
    'CONTEST_ID',
    'STX_STRING',
    'SRX_STRING'
]);

export const EXTRA_FIELD_NAMES = ADIF_QSO_FIELDS.filter(
    (name) => !ENTERED.has(name) && name !== 'STATION_CALLSIGN'
);

export function extraFieldProblem(name: string): string | undefined {
    if (ENTERED.has(name)) return `${name} has its own field.`;
    if (name === 'STATION_CALLSIGN') return 'STATION_CALLSIGN comes from the Station.';
    if (!isAdifQsoField(name)) return `${name} is not an ADIF QSO field.`;
    return undefined;
}

function filledExtras(entry: Entry): ExtraField[] {
    return entry.extra
        .map(({ name, value }) => ({ name: name.trim().toUpperCase(), value: value.trim() }))
        .filter(({ value }) => value !== '');
}

export function entryProblem(entry: Entry, contest: ContestSettings): string | undefined {
    if (entry.call.trim() === '') return 'Enter the call.';
    if (entry.band === '' || entry.mode === '') return 'Choose the band and the mode.';
    const problem = contestProblem(contest);
    if (problem !== undefined) return problem;
    if (contest.enabled && entry.received.trim() === '')
        return 'Contest: the received number is required.';
    const power = entry.txPwr.trim();
    if (power !== '' && !isPower(power)) return POWER_RULE;
    const seen = new Set<string>();
    for (const { name } of filledExtras(entry)) {
        if (name === '') return 'Name the extra field, or clear its value.';
        const extra = extraFieldProblem(name);
        if (extra !== undefined) return extra;
        if (seen.has(name)) return `${name} is entered twice.`;
        seen.add(name);
    }
    return undefined;
}

export function entryFields(entry: Entry, contest: ContestSettings, at: Date): Fields {
    const fields: Fields = {
        ...Object.fromEntries(filledExtras(entry).map(({ name, value }) => [name, value])),
        CALL: entry.call.trim().toUpperCase(),
        ...utcFields(at),
        FREQ: entry.freq.trim(),
        BAND: entry.band,
        MODE: entry.mode,
        SUBMODE: entry.submode,
        RST_SENT: entry.rstSent.trim(),
        RST_RCVD: entry.rstRcvd.trim(),
        NAME: entry.name.trim(),
        QTH: entry.qth.trim(),
        CNTY: entry.cnty.trim(),
        ...qslRequestFields(entry.qsl),
        COMMENT: entry.comment.trim(),
        TX_PWR: entry.txPwr.trim(),
        NOTES: entry.notes.trim()
    };
    if (contest.enabled) {
        const sent = expandExchange(contest.exchangeTemplate, contest.nextSerial);
        fields.CONTEST_ID = contest.contestId.trim().toUpperCase();
        fields.STX_STRING = exchangeString(contest, fields.RST_SENT, sent);
        fields.SRX_STRING = exchangeString(
            contest,
            fields.RST_RCVD,
            entry.received.trim().toUpperCase()
        );
    }
    return Object.fromEntries(Object.entries(fields).filter(([, v]) => v !== ''));
}
