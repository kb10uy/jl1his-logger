import type { Fields } from '#lib/api/types.ts';
import { adifDate, adifTime, hhmmss, isoDate } from './format.ts';

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
    comment: string;
    qslSent: string;
    qslRcvd: string;
    qslSentVia: string;
    qslSentDate: string;
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
    'QSL_SENT',
    'QSL_RCVD',
    'QSL_SENT_VIA',
    'QSLSDATE'
];

export function draftOf(f: Fields): Draft {
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
        comment: f.COMMENT ?? '',
        qslSent: (f.QSL_SENT ?? '').toUpperCase(),
        qslRcvd: (f.QSL_RCVD ?? '').toUpperCase(),
        qslSentVia: (f.QSL_SENT_VIA ?? '').toUpperCase(),
        qslSentDate: isoDate(f.QSLSDATE)
    };
}

function fieldsOfDraft(draft: Draft): Fields {
    return {
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
        QSL_SENT: draft.qslSent,
        QSL_RCVD: draft.qslRcvd,
        QSL_SENT_VIA: draft.qslSentVia,
        QSLSDATE: adifDate(draft.qslSentDate)
    };
}

export function fieldsOf(draft: Draft, original: Fields): Fields {
    const edited = fieldsOfDraft(draft);
    const before = fieldsOfDraft(draftOf(original));
    const kept = Object.entries(original).filter(([name]) => !EDITED.includes(name));
    const changed = Object.entries(edited)
        .map(([name, value]) => [name, value === before[name] ? (original[name] ?? '') : value])
        .filter(([, value]) => value !== '');
    return Object.fromEntries([...kept, ...changed]);
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
