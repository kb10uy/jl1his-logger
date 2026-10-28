import type { Area, Fields } from '#lib/api/types.ts';
import { m } from '#lib/paraglide/messages.js';
import { ADIF_QSO_FIELDS, isAdifQsoField } from '#lib/qso/adif-fields.ts';
import {
    contestProblem,
    exchangeString,
    expandExchange,
    type ContestSettings
} from '#lib/qso/contest.ts';
import { utcFields } from '#lib/qso/format.ts';
import { defaultRst } from '#lib/qso/modes.ts';
import { isPower, powerRule } from '#lib/qso/power.ts';
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
    area: string;
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
    'QSL_SENT',
    'QSL_RCVD',
    'COMMENT',
    'TX_PWR',
    'NOTES',
    'CONTEST_ID',
    'STX_STRING',
    'SRX_STRING'
]);

const entered = (name: string, area: Area) => ENTERED.has(name) || name === area?.name;

export function extraFieldNames(area: Area): string[] {
    return ADIF_QSO_FIELDS.filter((name) => !entered(name, area) && name !== 'STATION_CALLSIGN');
}

export function extraFieldProblem(name: string, area: Area): string | undefined {
    if (entered(name, area)) return m.entry_field_has_own({ name });
    if (name === 'STATION_CALLSIGN') return m.entry_field_from_station({ name });
    if (!isAdifQsoField(name)) return m.entry_field_unknown({ name });
    return undefined;
}

function filledExtras(entry: Entry): ExtraField[] {
    return entry.extra
        .map(({ name, value }) => ({ name: name.trim().toUpperCase(), value: value.trim() }))
        .filter(({ value }) => value !== '');
}

export function clearedEntry(entry: Entry): Entry {
    const rst = defaultRst(entry.mode, entry.submode);
    return {
        ...entry,
        call: '',
        rstSent: rst,
        rstRcvd: rst,
        received: '',
        name: '',
        qth: '',
        area: '',
        qsl: 'none',
        comment: '',
        notes: '',
        extra: []
    };
}

export function entryProblem(
    entry: Entry,
    contest: ContestSettings,
    area: Area = null
): string | undefined {
    if (entry.call.trim() === '') return m.entry_enter_call();
    if (entry.band === '' || entry.mode === '') return m.entry_choose_band_mode();
    const problem = contestProblem(contest);
    if (problem !== undefined) return problem;
    if (contest.enabled && entry.received.trim() === '') return m.entry_received_required();
    const power = entry.txPwr.trim();
    if (power !== '' && !isPower(power)) return powerRule();
    const seen = new Set<string>();
    for (const { name } of filledExtras(entry)) {
        if (name === '') return m.entry_name_extra_field();
        const extra = extraFieldProblem(name, area);
        if (extra !== undefined) return extra;
        if (seen.has(name)) return m.entry_field_twice({ name });
        seen.add(name);
    }
    return undefined;
}

export function entryFields(
    entry: Entry,
    contest: ContestSettings,
    at: Date,
    area: Area = null
): Fields {
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
        ...(area === null ? {} : { [area.name]: entry.area.trim() }),
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
