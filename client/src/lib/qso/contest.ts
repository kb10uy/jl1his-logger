export interface ContestSettings {
    enabled: boolean;
    contestId: string;
    exchangeTemplate: string;
    nextSerial: number;
    includeRst: boolean;
}

export const DEFAULT_SERIAL_WIDTH = 3;

export function defaultContest(): ContestSettings {
    return {
        enabled: false,
        contestId: '',
        exchangeTemplate: '{serial}',
        nextSerial: 1,
        includeRst: false
    };
}

const SERIAL_PLACEHOLDER = /\{serial(?::([1-9]))?\}/g;

export function usesSerial(template: string): boolean {
    return template.search(SERIAL_PLACEHOLDER) !== -1;
}

export function expandExchange(template: string, serial: number): string {
    return template
        .trim()
        .replace(SERIAL_PLACEHOLDER, (_, width?: string) =>
            String(serial).padStart(width === undefined ? DEFAULT_SERIAL_WIDTH : Number(width), '0')
        )
        .toUpperCase();
}

export function contestProblem(contest: ContestSettings): string | undefined {
    if (!contest.enabled) return undefined;
    const rest = contest.exchangeTemplate.replace(SERIAL_PLACEHOLDER, '');
    if (/[{}]/.test(rest)) return 'Sent number: only {serial} and {serial:N} can be used.';
    if (/\p{Cc}/u.test(rest)) return 'Sent number has control characters.';
    if (!Number.isSafeInteger(contest.nextSerial) || contest.nextSerial < 1)
        return 'Next serial must be 1 or more.';
    return undefined;
}

export function exchangeString(contest: ContestSettings, rst: string, exchange: string): string {
    return (contest.includeRst ? rst : '') + exchange;
}
