import type { Area, Fields } from '#lib/api/types.ts';
import {
    contestProblem,
    exchangeString,
    expandExchange,
    type ContestSettings
} from '#lib/qso/contest.ts';
import { qsoTime, utcFields } from '#lib/qso/format.ts';
import { qslRequestFields } from '#lib/qso/qsl.ts';
import { FREE_TEXT_MAX_LENGTH, type Draft } from './draft.ts';

const DUPLICATE_WINDOW_SECONDS = 15 * 60;

export function draftProblem(draft: Draft, contest: ContestSettings): string | undefined {
    if (draft.call === undefined) return 'Say the call first.';
    if (draft.band === '' || draft.mode === '') return 'Choose the band and the mode.';
    for (const [label, text] of [
        ['Name', draft.name],
        ['QTH', draft.qth]
    ] as const) {
        if (text !== undefined && [...text].length > FREE_TEXT_MAX_LENGTH)
            return `${label} is longer than ${FREE_TEXT_MAX_LENGTH} characters.`;
    }
    const problem = contestProblem(contest);
    if (problem !== undefined) return problem;
    if (contest.enabled && draft.received === undefined) return 'Contest: say the received number.';
    return undefined;
}

export function draftFields(
    draft: Draft,
    contest: ContestSettings,
    at: Date,
    area: Area = null
): Fields {
    const fields: Fields = {
        CALL: draft.call ?? '',
        ...utcFields(at),
        FREQ: draft.freq.trim(),
        BAND: draft.band,
        MODE: draft.mode,
        SUBMODE: draft.submode,
        RST_SENT: draft.rstSent,
        RST_RCVD: draft.rstRcvd,
        ...(area === null ? {} : { [area.name]: draft.jcx ?? '' }),
        ...qslRequestFields(draft.qsl),
        NAME: draft.name ?? '',
        QTH: draft.qth ?? ''
    };
    if (draft.received !== undefined)
        fields.SRX_STRING = exchangeString(contest, fields.RST_RCVD, draft.received);
    if (contest.enabled) {
        fields.CONTEST_ID = contest.contestId.trim().toUpperCase();
        fields.STX_STRING = exchangeString(
            contest,
            fields.RST_SENT,
            expandExchange(contest.exchangeTemplate, contest.nextSerial)
        );
    }
    return Object.fromEntries(Object.entries(fields).filter(([, v]) => v !== ''));
}

export function queuedDuplicate<T extends { fields: Fields }>(
    queued: readonly T[],
    candidate: Fields
): T | undefined {
    const same = (a: string | undefined, b: string | undefined) =>
        (a ?? '').trim().toUpperCase() === (b ?? '').trim().toUpperCase();
    const at = qsoTime(candidate);
    return queued.find(
        (item) =>
            same(item.fields.CALL, candidate.CALL) &&
            same(item.fields.BAND, candidate.BAND) &&
            same(item.fields.MODE, candidate.MODE) &&
            Math.abs(qsoTime(item.fields) - at) <= DUPLICATE_WINDOW_SECONDS
    );
}
