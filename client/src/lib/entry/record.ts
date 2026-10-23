import type { Fields } from '#lib/api/types.ts';
import {
    contestProblem,
    exchangeString,
    expandExchange,
    type ContestSettings
} from '#lib/qso/contest.ts';
import { utcFields } from '#lib/qso/format.ts';

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
    comment: string;
}

export function entryProblem(entry: Entry, contest: ContestSettings): string | undefined {
    if (entry.call.trim() === '') return 'Enter the call.';
    if (entry.band === '' || entry.mode === '') return 'Choose the band and the mode.';
    const problem = contestProblem(contest);
    if (problem !== undefined) return problem;
    if (contest.enabled && entry.received.trim() === '')
        return 'Contest: the received number is required.';
    return undefined;
}

export function entryFields(entry: Entry, contest: ContestSettings, at: Date): Fields {
    const fields: Fields = {
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
        COMMENT: entry.comment.trim()
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
