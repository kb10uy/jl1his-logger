import {
    COMMAND_KEYWORDS,
    DIGIT_WORDS,
    FREE_TEXT_ONLY_WORDS,
    HUNDRED_WORDS,
    JAPANESE_READINGS,
    LOG_KEYWORDS,
    MEGAHERTZ_WORDS,
    MODE_WORDS,
    PHONETIC_LETTERS,
    POINT_WORDS,
    QSL_VALUES,
    REPEAT_WORDS,
    STROKE_WORDS,
    SUFFIX_WORDS,
    TEEN_WORDS,
    TENS_WORDS,
    lookup,
    type CommandKind,
    type SpeechLanguage
} from './lexicon.ts';

type Slot =
    | 'letter'
    | 'number'
    | 'hundred'
    | 'repeat'
    | 'point'
    | 'megahertz'
    | 'stroke'
    | 'suffix'
    | 'callsignKeyword'
    | 'rstKeyword'
    | 'frequencyKeyword'
    | 'areaKeyword'
    | 'qslKeyword'
    | 'modeKeyword'
    | 'exchangeKeyword'
    | 'qslValue'
    | 'mode'
    | 'log'
    | 'unknown';

export type Sequence = readonly string[];

function single(words: Iterable<string>): Sequence[] {
    return [...words].filter((w) => !FREE_TEXT_ONLY_WORDS.has(w)).map((w) => [w]);
}

function keyword(...kinds: CommandKind[]): Sequence[] {
    return kinds.flatMap((kind) =>
        COMMAND_KEYWORDS[kind].filter((seq) => seq.every((w) => !FREE_TEXT_ONLY_WORDS.has(w)))
    );
}

const ENGLISH: Readonly<Record<Slot, readonly Sequence[]>> = {
    letter: [...single(Object.keys(PHONETIC_LETTERS)), ['x', 'ray']],
    number: single([
        ...Object.keys(DIGIT_WORDS),
        ...Object.keys(TEEN_WORDS),
        ...Object.keys(TENS_WORDS)
    ]),
    hundred: single(HUNDRED_WORDS),
    repeat: single(Object.keys(REPEAT_WORDS)),
    point: single(POINT_WORDS),
    megahertz: single(MEGAHERTZ_WORDS),
    stroke: single(STROKE_WORDS),
    suffix: single(Object.keys(SUFFIX_WORDS)),
    callsignKeyword: keyword('callsign'),
    rstKeyword: keyword('rstSent', 'rstReceived'),
    frequencyKeyword: keyword('frequency'),
    areaKeyword: keyword('area'),
    qslKeyword: keyword('qsl'),
    modeKeyword: keyword('mode'),
    exchangeKeyword: keyword('exchangeReceived'),
    qslValue: QSL_VALUES.map(({ words }) => words),
    mode: single(Object.keys(MODE_WORDS)),
    log: LOG_KEYWORDS,
    unknown: [['[unk]']]
};

const SLOTS = Object.keys(ENGLISH) as Slot[];

function inJapanese(sequence: Sequence): Sequence[] {
    const whole = lookup(JAPANESE_READINGS, sequence.join(' '));
    if (whole !== undefined) return [whole.split(' ')];
    const words = sequence.map((word) => lookup(JAPANESE_READINGS, word));
    return words.every((w) => w !== undefined) ? [words as string[]] : [];
}

const JAPANESE_SOURCE: Readonly<Record<Slot, readonly Sequence[]>> = {
    ...ENGLISH,
    letter: [
        ...ENGLISH.letter,
        ...[...new Set(Object.values(PHONETIC_LETTERS))].map((letter) => [letter.toLowerCase()])
    ]
};

const JAPANESE = Object.fromEntries(
    SLOTS.map((slot) => [
        slot,
        slot === 'unknown' ? ENGLISH.unknown : JAPANESE_SOURCE[slot].flatMap(inJapanese)
    ])
) as Record<Slot, readonly Sequence[]>;

export function grammarSequences(
    language: SpeechLanguage
): Readonly<Record<string, readonly Sequence[]>> {
    return language === 'ja' ? JAPANESE : ENGLISH;
}
const KEYWORDS: readonly Slot[] = [
    'callsignKeyword',
    'rstKeyword',
    'frequencyKeyword',
    'areaKeyword',
    'qslKeyword',
    'modeKeyword',
    'exchangeKeyword'
];
const NOT_LOG: readonly Slot[] = SLOTS.filter((slot) => slot !== 'log');

const NEXT_COMMAND: readonly Slot[] = [...KEYWORDS, 'unknown'];

type Next = Slot | 'end';

const TRANSITIONS: Readonly<Record<Slot, readonly Next[]>> = {
    letter: ['letter', 'number', 'stroke', 'suffix', ...NEXT_COMMAND, 'end'],
    number: [
        'number',
        'hundred',
        'repeat',
        'point',
        'megahertz',
        'stroke',
        'suffix',
        'letter',
        ...NEXT_COMMAND,
        'end'
    ],
    hundred: ['number', 'letter', 'point', 'megahertz', 'stroke', 'suffix', ...NEXT_COMMAND, 'end'],
    repeat: ['number'],
    point: ['number'],
    megahertz: [...NEXT_COMMAND, 'end'],
    stroke: ['letter', 'number'],
    suffix: ['number', ...NEXT_COMMAND, 'end'],
    callsignKeyword: ['letter', 'number', 'unknown'],
    rstKeyword: ['number', 'unknown'],
    frequencyKeyword: ['number', 'point', 'unknown'],
    areaKeyword: ['letter', 'number', 'unknown'],
    qslKeyword: ['qslValue', 'unknown'],
    qslValue: [...NEXT_COMMAND, 'end'],
    modeKeyword: ['mode', 'letter', 'number', 'unknown'],
    exchangeKeyword: ['letter', 'number', 'unknown'],
    mode: [...NEXT_COMMAND, 'end'],
    log: ['end'],
    unknown: [...NOT_LOG, 'end']
};

const START: readonly Slot[] = ['letter', 'number', ...KEYWORDS, 'log', 'unknown'];

function prefixes(): Map<Slot, Slot[]> {
    const paths = new Map<Slot, Slot[]>();
    const queue: Slot[] = [];
    for (const slot of START) {
        paths.set(slot, []);
        queue.push(slot);
    }
    for (let i = 0; i < queue.length; i++) {
        const slot = queue[i];
        const path = [...paths.get(slot)!, slot];
        for (const next of TRANSITIONS[slot]) {
            if (next === 'end' || paths.has(next)) continue;
            paths.set(next, path);
            queue.push(next);
        }
    }
    return paths;
}

function suffixes(): Map<Slot, Slot[]> {
    const paths = new Map<Slot, Slot[]>();
    const queue: Slot[] = [];
    for (const slot of SLOTS) {
        if (TRANSITIONS[slot].includes('end')) {
            paths.set(slot, []);
            queue.push(slot);
        }
    }
    for (let i = 0; i < queue.length; i++) {
        const slot = queue[i];
        const path = [slot, ...paths.get(slot)!];
        for (const prev of SLOTS) {
            if (paths.has(prev) || !TRANSITIONS[prev].includes(slot)) continue;
            paths.set(prev, path);
            queue.push(prev);
        }
    }
    return paths;
}

export function grammarPhrases(language: SpeechLanguage = 'en'): string[] {
    const words = language === 'ja' ? JAPANESE : ENGLISH;
    const prefix = prefixes();
    const suffix = suffixes();
    const counters = new Map<Slot, number>();
    const pick = (slot: Slot): Sequence => {
        const seqs = words[slot];
        const n = counters.get(slot) ?? 0;
        counters.set(slot, n + 1);
        return seqs[n % seqs.length];
    };

    const phrases = new Set<string>();
    const emit = (before: Slot[], words: Sequence[], after: Slot[]) => {
        phrases.add([...before.map(pick), ...words, ...after.map(pick)].flat().join(' '));
    };

    for (const slot of START) {
        for (const seq of words[slot]) emit([], [seq], suffix.get(slot)!);
    }
    for (const slot of SLOTS) {
        for (const next of TRANSITIONS[slot]) {
            if (next === 'end') {
                for (const seq of words[slot]) emit(prefix.get(slot)!, [seq], []);
                continue;
            }
            for (const seq of words[slot]) {
                for (const nextSeq of words[next]) {
                    emit(prefix.get(slot)!, [seq, nextSeq], suffix.get(next)!);
                }
            }
        }
    }
    return [...phrases];
}
