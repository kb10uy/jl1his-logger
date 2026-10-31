import type { BulkChange, Fields, Qso, Target } from '#lib/api/types.ts';
import { compareNewestFirst } from '#lib/qso/format.ts';

export interface Row {
    qso: Qso;
    stale: boolean;
}

export type Applied =
    | { kind: 'updated'; rows: Row[] }
    | { kind: 'inserted'; rows: Row[] }
    | { kind: 'outside' }
    | { kind: 'ignored' };

export function applyEvent(
    rows: Row[],
    qso: Qso,
    matches: boolean,
    complete: boolean,
    compare: (a: Qso, b: Qso) => number = compareNewestFirst
): Applied {
    const at = rows.findIndex((r) => r.qso.id === qso.id);
    const live = qso.deleted_at === null;
    if (at >= 0) {
        if (rows[at].qso.updated_seq >= qso.updated_seq) return { kind: 'ignored' };
        const next = [...rows];
        next[at] = { qso, stale: !live || !matches };
        return { kind: 'updated', rows: next };
    }
    if (!live) return { kind: 'ignored' };
    if (!matches) return qso.revision === 1 ? { kind: 'outside' } : { kind: 'ignored' };
    const row = { qso, stale: false };
    const before = rows.findIndex((r) => compare(row.qso, r.qso) < 0);
    if (before < 0) {
        return complete ? { kind: 'inserted', rows: [...rows, row] } : { kind: 'ignored' };
    }
    return { kind: 'inserted', rows: [...rows.slice(0, before), row, ...rows.slice(before)] };
}

export function replaceRows(rows: Row[], updated: Qso[], matches: (qso: Qso) => boolean): Row[] {
    const byId = new Map(updated.map((q) => [q.id, q]));
    return rows.map((r) => {
        const qso = byId.get(r.qso.id);
        if (qso === undefined || qso.updated_seq < r.qso.updated_seq) return r;
        return { qso, stale: qso.deleted_at !== null || !matches(qso) };
    });
}

export interface Undo {
    qsos: Target[];
    change: BulkChange;
}

export function undoOf(before: Qso[], after: Qso[]): Undo[] | undefined {
    const previous = new Map(before.map((q) => [q.id, q]));
    const groups = new Map<string, Undo>();
    for (const qso of after) {
        const old = previous.get(qso.id);
        if (old === undefined) return undefined;
        const names = new Set([...Object.keys(old.fields), ...Object.keys(qso.fields)]);
        const fields: Fields = {};
        for (const name of [...names].sort()) {
            if (old.fields[name] !== qso.fields[name]) fields[name] = old.fields[name] ?? '';
        }
        const change: BulkChange = { fields };
        if (old.station_id !== qso.station_id) change.station_id = old.station_id;
        if (old.instrument_id !== qso.instrument_id) change.instrument_id = old.instrument_id;
        const key = JSON.stringify(change);
        const group = groups.get(key) ?? { qsos: [], change };
        group.qsos.push({ id: qso.id, revision: qso.revision });
        groups.set(key, group);
    }
    return [...groups.values()];
}

export function tally(values: (string | undefined)[]): string {
    const counts = new Map<string, number>();
    for (const value of values) {
        const v = (value ?? '').trim() || '—';
        counts.set(v, (counts.get(v) ?? 0) + 1);
    }
    return [...counts].map(([v, n]) => `${v} ×${n}`).join(' · ') || '—';
}

export const distribution = (qsos: Qso[], name: string) => tally(qsos.map((q) => q.fields[name]));
