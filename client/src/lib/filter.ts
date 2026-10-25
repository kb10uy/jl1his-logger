export function matchesFilter(values: (string | null | undefined)[], query: string): boolean {
    const haystack = values
        .filter((v) => v !== null && v !== undefined)
        .map((v) => v.toLowerCase());
    return query
        .toLowerCase()
        .split(/\s+/)
        .filter((term) => term !== '')
        .every((term) => haystack.some((v) => v.includes(term)));
}
