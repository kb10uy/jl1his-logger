import { m } from '#lib/paraglide/messages.js';
import { formatTokens, letterOf, tokenLabel } from './display.ts';
import { DslError } from './errors.ts';
import { MAX_FRACTION_DIGITS, MAX_INTEGER_DIGITS, type FrequencyPattern } from './frequency.ts';
import { matchKeyword } from './keywords.ts';
import {
    LOG_KEYWORDS,
    MEGAHERTZ_WORDS,
    MODE_WORDS,
    POINT_WORDS,
    QSL_VALUES,
    STROKE_WORDS,
    SUFFIX_WORDS,
    lookup,
    type CommandKind,
    type QslStatus
} from './lexicon.ts';
import { KNOWN_MODES, isKnownModeName, type QsoMode } from './mode.ts';
import { isNumberToken, readDigits } from './numbers.ts';
import { tokenize } from './tokenize.ts';

export type SpokenUpdate =
    | { kind: 'callsign'; value: string }
    | { kind: 'rstSent'; value: string }
    | { kind: 'rstReceived'; value: string }
    | { kind: 'frequency'; value: FrequencyPattern }
    | { kind: 'area'; value: string }
    | { kind: 'qsl'; value: QslStatus }
    | { kind: 'mode'; value: QsoMode }
    | { kind: 'exchangeReceived'; value: string };

export type ParseResult =
    | { ok: true; tokens: string[]; updates: SpokenUpdate[]; log: boolean }
    | { ok: false; tokens: string[]; error: string };

const POINTS = new Set<string>(POINT_WORDS);
const MEGAHERTZ = new Set<string>(MEGAHERTZ_WORDS);
const STROKES = new Set<string>(STROKE_WORDS);

function logAt(tokens: readonly string[], i: number): readonly string[] | undefined {
    return LOG_KEYWORDS.find((seq) => seq.every((word, k) => tokens[i + k] === word));
}

function readSpelled(tokens: readonly string[], allowStroke: boolean): string {
    let out = '';
    let i = 0;
    while (i < tokens.length) {
        const token = tokens[i];
        const letter = letterOf(token);
        const suffix = allowStroke ? lookup(SUFFIX_WORDS, token) : undefined;
        if (letter !== undefined) {
            out += letter;
            i += 1;
        } else if (isNumberToken(token)) {
            const { digits, end } = readDigits(tokens, i);
            if (end === i) throw new DslError(m.voice_dsl_unexpected({ token: tokenLabel(token) }));
            out += digits;
            i = end;
        } else if (allowStroke && STROKES.has(token)) {
            out += '/';
            i += 1;
        } else if (suffix !== undefined) {
            const next = tokens[i + 1];
            out += next !== undefined && isNumberToken(next) ? '/' : suffix;
            i += 1;
        } else {
            throw new DslError(m.voice_dsl_unexpected({ token: tokenLabel(token) }));
        }
    }
    return out;
}

const CALLSIGN_PATTERN = /^(?=.*[A-Z])(?=.*\d)[A-Z0-9]+(?:\/[A-Z0-9]+)*$/;

export function isCallsign(s: string): boolean {
    return s.length >= 3 && s.length <= 16 && CALLSIGN_PATTERN.test(s);
}

function parseCallsign(tokens: readonly string[]): string {
    if (tokens.length === 0) throw new DslError(m.voice_dsl_callsign_empty());
    const callsign = readSpelled(tokens, true);
    if (!isCallsign(callsign)) throw new DslError(m.voice_dsl_callsign_invalid({ callsign }));
    return callsign;
}

function readAllDigits(tokens: readonly string[], what: string): string {
    const { digits, end } = readDigits(tokens, 0);
    if (end !== tokens.length)
        throw new DslError(m.voice_dsl_unexpected_in({ token: tokenLabel(tokens[end]), what }));
    if (digits === '') throw new DslError(m.voice_dsl_empty({ what }));
    return digits;
}

function parseRst(tokens: readonly string[]): string {
    const rst = readAllDigits(tokens, 'RST');
    if (!/^[1-5][1-9][1-9]?$/.test(rst)) throw new DslError(m.voice_dsl_rst_invalid({ rst }));
    return rst;
}

function parseFrequency(tokens: readonly string[]): FrequencyPattern {
    const integer = readDigits(tokens, 0);
    let i = integer.end;
    let fractionDigits = '';
    if (i < tokens.length && POINTS.has(tokens[i])) {
        const fraction = readDigits(tokens, i + 1);
        if (fraction.end === i + 1) throw new DslError(m.voice_dsl_digits_after_point());
        fractionDigits = fraction.digits;
        i = fraction.end;
    }
    if (i < tokens.length && MEGAHERTZ.has(tokens[i])) i += 1;
    if (i !== tokens.length)
        throw new DslError(m.voice_dsl_frequency_unexpected({ token: tokenLabel(tokens[i]) }));
    if (integer.digits === '' && fractionDigits === '')
        throw new DslError(m.voice_dsl_frequency_empty());
    if (integer.digits.length > MAX_INTEGER_DIGITS || fractionDigits.length > MAX_FRACTION_DIGITS) {
        throw new DslError(m.voice_dsl_frequency_too_many_digits());
    }
    return { integerDigits: integer.digits, fractionDigits };
}

const AREA_MAX_LENGTH = 16;

function parseArea(tokens: readonly string[]): string {
    if (tokens.length === 0) throw new DslError(m.voice_dsl_area_empty());
    const area = readSpelled(tokens, false);
    if (area.length > AREA_MAX_LENGTH) throw new DslError(m.voice_dsl_area_too_long());
    return area;
}

function parseQsl(tokens: readonly string[]): QslStatus {
    const value = QSL_VALUES.find(
        ({ words }) => words.length === tokens.length && words.every((w, i) => w === tokens[i])
    );
    if (value !== undefined) return value.status;
    const expected = QSL_VALUES.map(({ words }) => `"${words.join(' ')}"`).join(', ');
    throw new DslError(m.voice_dsl_qsl_expected({ expected, got: formatTokens(tokens) }));
}

function parseMode(tokens: readonly string[]): QsoMode {
    if (tokens.length === 0) throw new DslError(m.voice_dsl_mode_empty());
    const name =
        (tokens.length === 1 ? lookup(MODE_WORDS, tokens[0]) : undefined) ??
        readSpelled(tokens, false);
    if (!isKnownModeName(name)) throw new DslError(m.voice_dsl_mode_unknown({ mode: name }));
    return { ...KNOWN_MODES[name] };
}

const EXCHANGE_MAX_LENGTH = 16;

function parseExchange(tokens: readonly string[]): string {
    if (tokens.length === 0) throw new DslError(m.voice_dsl_number_empty());
    const exchange = readSpelled(tokens, false);
    if (exchange.length > EXCHANGE_MAX_LENGTH) throw new DslError(m.voice_dsl_number_too_long());
    return exchange;
}

function parseSegment(kind: CommandKind, tokens: readonly string[]): SpokenUpdate {
    switch (kind) {
        case 'callsign':
            return { kind: 'callsign', value: parseCallsign(tokens) };
        case 'rstSent':
            return { kind: 'rstSent', value: parseRst(tokens) };
        case 'rstReceived':
            return { kind: 'rstReceived', value: parseRst(tokens) };
        case 'frequency':
            return { kind: 'frequency', value: parseFrequency(tokens) };
        case 'area':
            return { kind: 'area', value: parseArea(tokens) };
        case 'qsl':
            return { kind: 'qsl', value: parseQsl(tokens) };
        case 'mode':
            return { kind: 'mode', value: parseMode(tokens) };
        case 'exchangeReceived':
            return { kind: 'exchangeReceived', value: parseExchange(tokens) };
    }
}

export function parseTokens(tokens: readonly string[]): SpokenUpdate[] {
    if (tokens.length === 0) throw new DslError(m.voice_dsl_nothing_recognized());

    const segments: { kind: CommandKind; tokens: string[] }[] = [];
    let current: { kind: CommandKind; tokens: string[] } = { kind: 'callsign', tokens: [] };
    let i = 0;
    while (i < tokens.length) {
        const match = matchKeyword(tokens, i);
        if (match === null) {
            current.tokens.push(tokens[i]);
            i += 1;
            continue;
        }
        if (segments.length > 0 || current.tokens.length > 0) segments.push(current);
        current = { kind: match.kind, tokens: [] };
        i += match.length;
    }
    segments.push(current);

    return segments.map((s) => parseSegment(s.kind, s.tokens));
}

export function parseSpeech(text: string): ParseResult {
    const tokens = tokenize(text);
    try {
        if (tokens.some((_, i) => logAt(tokens, i) !== undefined)) {
            if (logAt(tokens, 0)?.length !== tokens.length)
                throw new DslError(m.voice_dsl_qso_alone());
            return { ok: true, tokens, updates: [], log: true };
        }
        return { ok: true, tokens, updates: parseTokens(tokens), log: false };
    } catch (e) {
        if (e instanceof DslError) return { ok: false, tokens, error: e.message };
        throw e;
    }
}
