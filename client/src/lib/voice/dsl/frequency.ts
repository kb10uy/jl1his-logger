export interface FrequencyPattern {
    integerDigits: string;
    fractionDigits: string;
}

export interface FrequencyContext {
    anchorHz: number;
    currentHz?: number;
}

const HZ_PER_MHZ = 1_000_000;
export const MAX_INTEGER_DIGITS = 5;
export const MAX_FRACTION_DIGITS = 6;

export function formatFrequencyPattern(pattern: FrequencyPattern): string {
    const fraction = pattern.fractionDigits === '' ? '' : `.${pattern.fractionDigits}`;
    return `*${pattern.integerDigits}${fraction}`;
}

export function resolveFrequency(pattern: FrequencyPattern, context: FrequencyContext): number {
    const { integerDigits, fractionDigits } = pattern;
    if (!/^\d*$/.test(integerDigits) || !/^\d*$/.test(fractionDigits)) {
        throw new RangeError('frequency pattern must only contain digits');
    }
    if (integerDigits.length > MAX_INTEGER_DIGITS || fractionDigits.length > MAX_FRACTION_DIGITS) {
        throw new RangeError('frequency pattern has too many digits');
    }
    if (!Number.isFinite(context.anchorHz) || context.anchorHz <= 0) {
        throw new RangeError('frequency anchor must be a positive number');
    }

    const step = 10 ** integerDigits.length * HZ_PER_MHZ;
    const integerHz = integerDigits === '' ? 0 : Number(integerDigits) * HZ_PER_MHZ;
    const fractionHz =
        fractionDigits === '' ? 0 : Number(fractionDigits.padEnd(MAX_FRACTION_DIGITS, '0'));
    const offset = integerHz + fractionHz;

    let candidate = offset + Math.round((context.anchorHz - offset) / step) * step;
    while (candidate <= 0) candidate += step;
    return candidate;
}
