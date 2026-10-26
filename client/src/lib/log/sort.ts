import type { Qso } from '#lib/api/types.ts';
import { compareNewestFirst } from '#lib/qso/format.ts';
import { callBase } from '#lib/query/index.ts';

export const SORTS = ['-time', 'time', 'call', '-call'] as const;

export type Sort = (typeof SORTS)[number];

export const DEFAULT_SORT: Sort = '-time';

export function parseSort(value: string | null): Sort {
    return SORTS.find((s) => s === value) ?? DEFAULT_SORT;
}

export type SortColumn = 'time' | 'call';

export function isSortColumn(id: string): id is SortColumn {
    return id === 'time' || id === 'call';
}

export function directionOf(
    column: SortColumn,
    sort: Sort
): 'ascending' | 'descending' | undefined {
    if (sort === column) return 'ascending';
    if (sort === `-${column}`) return 'descending';
    return undefined;
}

export function toggled(column: SortColumn, sort: Sort): Sort {
    const first: Sort = column === 'time' ? '-time' : 'call';
    if (sort !== first) return first;
    return column === 'time' ? 'time' : '-call';
}

function compareCalls(a: Qso, b: Qso): number {
    const x = callBase(a.fields.CALL ?? '');
    const y = callBase(b.fields.CALL ?? '');
    return x < y ? -1 : x > y ? 1 : 0;
}

export function comparator(sort: Sort): (a: Qso, b: Qso) => number {
    switch (sort) {
        case '-time':
            return compareNewestFirst;
        case 'time':
            return (a, b) => compareNewestFirst(b, a);
        case 'call':
            return (a, b) => compareCalls(a, b) || compareNewestFirst(a, b);
        case '-call':
            return (a, b) => compareCalls(b, a) || compareNewestFirst(a, b);
    }
}
