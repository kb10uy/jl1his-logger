import init, { analyze as analyzeJson, callBase as callBaseOf, Matcher } from './wasm/query.js';
import type { Fields } from '#lib/api/types.ts';

export interface QueryToken {
    start: number;
    end: number;
    text: string;
    negated: boolean;
    key: string | null;
    field: string | null;
    values: string[];
    error: { code: string; message: string } | null;
}

let loading: Promise<void> | undefined;

export function loadQuery(): Promise<void> {
    loading ??= init().then(() => undefined);
    return loading;
}

export function analyze(query: string): QueryToken[] {
    return JSON.parse(analyzeJson(query)) as QueryToken[];
}

export function callBase(call: string): string {
    return callBaseOf(call);
}

export interface MatchedRow {
    fields: Fields;
    stationId?: string;
    stationName?: string;
    instrumentId?: string | null;
    instrumentName?: string;
}

export interface RowMatcher {
    matches(row: MatchedRow): boolean;
}

export function matcher(query: string): RowMatcher | undefined {
    try {
        const compiled = new Matcher(query);
        return {
            matches: (row) =>
                compiled.matches(
                    row.fields,
                    row.stationId,
                    row.stationName,
                    row.instrumentId ?? undefined,
                    row.instrumentName
                )
        };
    } catch {
        return undefined;
    }
}

export function removeToken(query: string, token: QueryToken): string {
    const chars = Array.from(query);
    return [...chars.slice(0, token.start), ...chars.slice(token.end)]
        .join('')
        .replace(/\s+/g, ' ')
        .trim();
}

export function appendTerm(query: string, term: string): string {
    return `${query.trim()} ${term}`.trim();
}
