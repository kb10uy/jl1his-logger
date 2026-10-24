import type { KnownModeName } from './mode.ts';

export function lookup<T>(
    table: Readonly<Record<string, T>>,
    token: string | undefined
): T | undefined {
    return token !== undefined && Object.hasOwn(table, token) ? table[token] : undefined;
}

export const PHONETIC_LETTERS: Readonly<Record<string, string>> = {
    alpha: 'A',
    alfa: 'A',
    bravo: 'B',
    charlie: 'C',
    delta: 'D',
    echo: 'E',
    foxtrot: 'F',
    fox: 'F',
    golf: 'G',
    hotel: 'H',
    india: 'I',
    juliett: 'J',
    juliet: 'J',
    juliette: 'J',
    kilo: 'K',
    lima: 'L',
    mike: 'M',
    november: 'N',
    oscar: 'O',
    papa: 'P',
    quebec: 'Q',
    romeo: 'R',
    sierra: 'S',
    tango: 'T',
    uniform: 'U',
    victor: 'V',
    whiskey: 'W',
    whisky: 'W',
    xray: 'X',
    'x-ray': 'X',
    yankee: 'Y',
    zulu: 'Z'
};

export const DIGIT_WORDS: Readonly<Record<string, string>> = {
    zero: '0',
    oh: '0',
    one: '1',
    won: '1',
    two: '2',
    to: '2',
    too: '2',
    three: '3',
    tree: '3',
    four: '4',
    for: '4',
    fower: '4',
    five: '5',
    fife: '5',
    six: '6',
    seven: '7',
    eight: '8',
    ate: '8',
    nine: '9',
    niner: '9'
};

export const TEEN_WORDS: Readonly<Record<string, string>> = {
    ten: '10',
    eleven: '11',
    twelve: '12',
    thirteen: '13',
    fourteen: '14',
    fifteen: '15',
    sixteen: '16',
    seventeen: '17',
    eighteen: '18',
    nineteen: '19'
};

export const TENS_WORDS: Readonly<Record<string, string>> = {
    twenty: '2',
    thirty: '3',
    forty: '4',
    fourty: '4',
    fifty: '5',
    sixty: '6',
    seventy: '7',
    eighty: '8',
    ninety: '9'
};

export const HUNDRED_WORDS = ['hundred'] as const;
export const REPEAT_WORDS: Readonly<Record<string, number>> = { double: 2, triple: 3 };
export const POINT_WORDS = ['point', 'decimal', 'dot'] as const;
export const MEGAHERTZ_WORDS = ['megahertz', 'mhz', 'meg', 'megs'] as const;
export const STROKE_WORDS = ['stroke', 'slash'] as const;
export const SUFFIX_WORDS: Readonly<Record<string, string>> = { portable: '/P', mobile: '/M' };

export const FREE_TEXT_ONLY_WORDS: ReadonlySet<string> = new Set([
    'alfa',
    'fox',
    'juliett',
    'juliette',
    'whisky',
    'xray',
    'oh',
    'won',
    'to',
    'too',
    'for',
    'ate',
    'fourty',
    'rcvd',
    'nr'
]);

export const FILLER_WORDS = ['the', 'uh', 'um', 'er', 'ah', 'and', 'is', '[unk]'] as const;

export const COMMAND_KEYWORDS = {
    callsign: [['call'], ['callsign'], ['call', 'sign'], ['station']],
    rstSent: [['sent'], ['send'], ['sending']],
    rstReceived: [['received'], ['receive'], ['receiving'], ['rcvd']],
    frequency: [['frequency'], ['frequencies'], ['freq']],
    area: [['area']],
    qsl: [['card'], ['qsl'], ['q', 's', 'l']],
    mode: [['mode']],
    exchangeReceived: [['number'], ['nr']]
} as const satisfies Record<string, readonly (readonly string[])[]>;

export type CommandKind = keyof typeof COMMAND_KEYWORDS;

export const COMMAND_LABELS: Readonly<Record<CommandKind, string>> = {
    callsign: 'CALL',
    rstSent: 'SENT',
    rstReceived: 'RCVD',
    frequency: 'FREQ',
    area: 'AREA',
    qsl: 'QSL',
    mode: 'MODE',
    exchangeReceived: 'NR'
};

export const LOG_KEYWORDS: readonly (readonly string[])[] = [['qso'], ['q', 's', 'o']];

export type QslStatus = 'none' | 'requested' | 'oneWay';

export const QSL_VALUES: readonly {
    readonly words: readonly string[];
    status: QslStatus;
    label: string;
}[] = [
    { words: ['requested'], status: 'requested', label: 'REQUESTED' },
    { words: ['one', 'way'], status: 'oneWay', label: 'ONE WAY' },
    { words: ['negative'], status: 'none', label: 'NEGATIVE' }
];

export const MODE_WORDS: Readonly<Record<string, KnownModeName>> = {
    fm: 'FM',
    am: 'AM',
    ssb: 'SSB',
    cw: 'CW',
    usb: 'USB',
    lsb: 'LSB',
    rtty: 'RTTY',
    ft8: 'FT8',
    ft4: 'FT4'
};

export type SpeechLanguage = 'en' | 'ja';

export const JAPANESE_READINGS: Readonly<Record<string, string>> = {
    alpha: 'アルファ',
    bravo: 'ブラボー',
    charlie: 'チャーリー',
    delta: 'デルタ',
    echo: 'エコー',
    foxtrot: 'フォックス トロット',
    golf: 'ゴルフ',
    hotel: 'ホテル',
    india: 'インディア',
    juliet: 'ジュリエット',
    kilo: 'キロ',
    lima: 'リマ',
    mike: 'マイク',
    november: 'ＮＯＶＥＭＢＥＲ',
    oscar: 'オスカー',
    papa: 'パパ',
    quebec: 'ケベック',
    romeo: 'ロメオ',
    sierra: 'シエラ',
    tango: 'タンゴ',
    uniform: 'ユニフォーム',
    victor: 'ビクター',
    whiskey: 'ウイスキー',
    'x ray': 'Ｘ－ｒａｙ',
    yankee: 'ヤンキー',
    zulu: 'ズール',

    a: 'Ａ',
    b: 'Ｂ',
    c: 'Ｃ',
    d: 'Ｄ',
    e: 'Ｅ',
    f: 'Ｆ',
    g: 'Ｇ',
    h: 'Ｈ',
    i: 'Ｉ',
    j: 'Ｊ',
    k: 'Ｋ',
    l: 'Ｌ',
    m: 'Ｍ',
    n: 'Ｎ',
    o: 'Ｏ',
    p: 'Ｐ',
    q: 'Ｑ',
    r: 'Ｒ',
    s: 'Ｓ',
    t: 'Ｔ',
    u: 'Ｕ',
    v: 'Ｖ',
    w: 'Ｗ',
    x: 'Ｘ',
    y: 'Ｙ',
    z: 'Ｚ',

    zero: 'ゼロ',
    one: 'ワン',
    two: 'ツー',
    three: 'スリー',
    four: 'フォー',
    five: 'ファイブ',
    six: 'シックス',
    seven: 'セブン',
    eight: 'エイト',
    nine: 'ナイン',
    ten: 'テン',
    eleven: 'イレブン',
    twelve: 'トゥウェルブ',
    thirteen: 'サーティーン',
    fourteen: 'フォーティーン',
    fifteen: 'フィフティーン',
    sixteen: 'シックス ティーン',
    seventeen: 'セブンティーン',
    eighteen: 'エイ ティーン',
    nineteen: 'ナインティーン',
    twenty: 'トゥエンティー',
    thirty: 'サーティー',
    forty: 'ＦＯＲＴＹ',
    fifty: 'フィフティー',
    sixty: 'シックス ティ',
    seventy: 'セブンティー',
    eighty: 'エイティ',
    ninety: 'ナインティー',
    hundred: 'ハンドレッド',
    double: 'ダブル',
    triple: 'トリプル',

    point: 'ポイント',
    dot: 'ドット',
    megahertz: 'メガヘルツ',
    stroke: 'ストローク',
    slash: 'スラッシュ',
    portable: 'ポータブル',
    mobile: 'モバイル',

    call: 'コール',
    callsign: 'コールサイン',
    station: 'ステーション',
    sent: '送信',
    received: '受信',
    frequency: '周波数',
    area: 'エリア',
    card: 'カード',
    requested: 'リクエスト',
    'one way': 'ワンウェイ',
    negative: 'ネガティブ',
    mode: 'モード',
    number: 'ナンバー',
    qso: 'ＱＳＯ',

    fm: 'ＦＭ',
    am: 'ＡＭ',
    ssb: 'ＳＳＢ',
    cw: 'ＣＷ',
    usb: 'ＵＳＢ',
    lsb: 'ＬＳＢ',
    ft4: 'ＦＴ４'
};
