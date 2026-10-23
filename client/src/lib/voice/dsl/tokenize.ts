import {
    COMMAND_KEYWORDS,
    FILLER_WORDS,
    JAPANESE_READINGS,
    MODE_WORDS,
    PHONETIC_LETTERS,
    lookup
} from './lexicon.ts';

const KEYWORD_TOKENS = new Set<string>(
    Object.values(COMMAND_KEYWORDS).flatMap((seqs) => seqs.flatMap((seq) => [...seq]))
);
const FILLERS = new Set<string>(FILLER_WORDS);

const PHONETIC_OF_LETTER = new Map<string, string>();
for (const [word, letter] of Object.entries(PHONETIC_LETTERS)) {
    if (!PHONETIC_OF_LETTER.has(letter)) PHONETIC_OF_LETTER.set(letter, word);
}

const READINGS = Object.entries(JAPANESE_READINGS)
    .map(([english, reading]) => ({
        words: reading.normalize('NFKC').toLowerCase().split(' '),
        english
    }))
    .sort((a, b) => b.words.length - a.words.length);

function fromJapanese(words: readonly string[]): string[] {
    const out: string[] = [];
    let i = 0;
    while (i < words.length) {
        const match = READINGS.find((r) => r.words.every((w, k) => words[i + k] === w));
        out.push(match?.english ?? words[i]);
        i += match?.words.length ?? 1;
    }
    return out;
}

export function tokenize(text: string): string[] {
    const words = text
        .normalize('NFKC')
        .toLowerCase()
        .replace(/[,;:!?"()、。・]/g, ' ')
        .split(/\s+/)
        .filter((w) => w !== '');
    const normalized = fromJapanese(words)
        .join(' ')
        .replace(/\bx[\s-]+ray\b/g, 'xray');

    const tokens: string[] = [];
    for (const raw of normalized.split(/\s+/)) {
        if (raw === '') continue;
        const word = raw.replace(/^(?:[.'-](?!\d))+|[.'-]+$/g, '');
        if (word === '') {
            if (raw.includes('.')) tokens.push('point');
            continue;
        }
        if (FILLERS.has(word)) continue;
        if (KEYWORD_TOKENS.has(word) || lookup(MODE_WORDS, word) !== undefined) {
            tokens.push(word);
        } else if (/^[a-z0-9./]+$/.test(word) && /\d/.test(word)) {
            for (const ch of word) {
                if (ch === '.') tokens.push('point');
                else if (ch === '/') tokens.push('stroke');
                else tokens.push(PHONETIC_OF_LETTER.get(ch.toUpperCase()) ?? ch);
            }
        } else {
            for (const part of word.split('-')) if (part !== '') tokens.push(part);
        }
    }
    return tokens;
}
