import type { Area, Fields } from '#lib/api/types.ts';
import { ADIF_QSO_FIELDS, isAdifQsoField } from './adif-fields.ts';
import { adifDate, adifTime, hhmmss, isoDate } from './format.ts';
import { isPower, POWER_RULE } from './power.ts';

export interface ExtraField {
    name: string;
    value: string;
}

export interface Draft {
    call: string;
    date: string;
    time: string;
    freq: string;
    band: string;
    mode: string;
    submode: string;
    rstSent: string;
    rstRcvd: string;
    name: string;
    qth: string;
    area: string;
    comment: string;
    notes: string;
    txPwr: string;
    qslSent: string;
    qslRcvd: string;
    qslSentVia: string;
    qslSentDate: string;
    qslRcvdVia: string;
    qslRcvdDate: string;
    extra: ExtraField[];
}

export const EDITED = [
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
    'COMMENT',
    'NOTES',
    'TX_PWR',
    'QSL_SENT',
    'QSL_RCVD',
    'QSL_SENT_VIA',
    'QSLSDATE',
    'QSL_RCVD_VIA',
    'QSLRDATE'
];

export function isStationField(name: string): boolean {
    return name === 'STATION_CALLSIGN' || name === 'OPERATOR' || name.startsWith('MY_');
}

const edited = (name: string, area: Area) => EDITED.includes(name) || name === area?.name;

export function extraFieldNames(area: Area): string[] {
    return ADIF_QSO_FIELDS.filter((name) => !edited(name, area) && !isStationField(name));
}

export function extraFieldProblem(name: string, area: Area): string | undefined {
    if (edited(name, area)) return `${name} has its own field.`;
    if (isStationField(name)) return `${name} comes from the Station.`;
    if (!isAdifQsoField(name)) return `${name} is not an ADIF QSO field.`;
    return undefined;
}

export function draftOf(f: Fields, area: Area = null): Draft {
    return {
        call: f.CALL ?? '',
        date: isoDate(f.QSO_DATE),
        time: hhmmss(f.TIME_ON),
        freq: f.FREQ ?? '',
        band: f.BAND ?? '',
        mode: f.MODE ?? '',
        submode: f.SUBMODE ?? '',
        rstSent: f.RST_SENT ?? '',
        rstRcvd: f.RST_RCVD ?? '',
        name: f.NAME ?? '',
        qth: f.QTH ?? '',
        area: area === null ? '' : (f[area.name] ?? ''),
        comment: f.COMMENT ?? '',
        notes: f.NOTES ?? '',
        txPwr: f.TX_PWR ?? '',
        qslSent: (f.QSL_SENT ?? '').toUpperCase(),
        qslRcvd: (f.QSL_RCVD ?? '').toUpperCase(),
        qslSentVia: (f.QSL_SENT_VIA ?? '').toUpperCase(),
        qslSentDate: isoDate(f.QSLSDATE),
        qslRcvdVia: (f.QSL_RCVD_VIA ?? '').toUpperCase(),
        qslRcvdDate: isoDate(f.QSLRDATE),
        extra: Object.entries(f)
            .filter(([name]) => !edited(name, area) && !isStationField(name))
            .map(([name, value]) => ({ name, value }))
    };
}

function fieldsOfDraft(draft: Draft, area: Area): Fields {
    return {
        ...(area === null ? {} : { [area.name]: draft.area.trim() }),
        CALL: draft.call.trim().toUpperCase(),
        QSO_DATE: adifDate(draft.date),
        TIME_ON: adifTime(draft.time),
        FREQ: draft.freq.trim(),
        BAND: draft.band,
        MODE: draft.mode,
        SUBMODE: draft.submode,
        RST_SENT: draft.rstSent.trim(),
        RST_RCVD: draft.rstRcvd.trim(),
        NAME: draft.name.trim(),
        QTH: draft.qth.trim(),
        COMMENT: draft.comment.trim(),
        NOTES: draft.notes.trim(),
        TX_PWR: draft.txPwr.trim(),
        QSL_SENT: draft.qslSent,
        QSL_RCVD: draft.qslRcvd,
        QSL_SENT_VIA: draft.qslSentVia,
        QSLSDATE: adifDate(draft.qslSentDate),
        QSL_RCVD_VIA: draft.qslRcvdVia,
        QSLRDATE: adifDate(draft.qslRcvdDate)
    };
}

function filledExtras(draft: Draft): ExtraField[] {
    return draft.extra
        .map(({ name, value }) => ({ name: name.trim().toUpperCase(), value: value.trim() }))
        .filter(({ value }) => value !== '');
}

export function fieldsOf(draft: Draft, original: Fields, area: Area = null): Fields {
    const edited = fieldsOfDraft(draft, area);
    const before = fieldsOfDraft(draftOf(original, area), area);
    const asBefore = (name: string, value: string) => {
        const kept = original[name];
        return kept !== undefined && value === kept.trim() ? kept : value;
    };
    const station = Object.entries(original).filter(([name]) => isStationField(name));
    const changed = Object.entries(edited)
        .map(([name, value]) => [name, value === before[name] ? (original[name] ?? '') : value])
        .filter(([, value]) => value !== '');
    const extras = filledExtras(draft).map(({ name, value }) => [name, asBefore(name, value)]);
    return Object.fromEntries([...station, ...changed, ...extras]);
}

export function missing(draft: Draft): string[] {
    const required: [string, string][] = [
        ['Call', draft.call.trim()],
        ['Date', draft.date],
        ['Time', draft.time],
        ['Band', draft.band],
        ['Mode', draft.mode]
    ];
    return required.filter(([, v]) => v === '').map(([label]) => label);
}

export function problem(draft: Draft, area: Area = null): string | undefined {
    const absent = missing(draft);
    if (absent.length > 0) return `Needed before saving: ${absent.join(', ')}.`;
    const power = draft.txPwr.trim();
    if (power !== '' && !isPower(power)) return POWER_RULE;
    const seen = new Set<string>();
    for (const { name } of filledExtras(draft)) {
        if (name === '') return 'Name the extra field, or clear its value.';
        const extra = extraFieldProblem(name, area);
        if (extra !== undefined) return extra;
        if (seen.has(name)) return `${name} is entered twice.`;
        seen.add(name);
    }
    return undefined;
}
