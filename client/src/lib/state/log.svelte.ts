import * as api from '#lib/api/api.ts';
import type { Qso } from '#lib/api/types.ts';
import { applyEvent, replaceRows, type Row } from '#lib/log/rows.ts';
import { DEFAULT_SORT, comparator, type Sort } from '#lib/log/sort.ts';
import { loadQuery, matcher } from '#lib/query/index.ts';
import type { LogbookState } from '#lib/state/logbook.svelte.ts';

const PAGE = 100;

export class LogList {
    q = $state('');
    sort = $state<Sort>(DEFAULT_SORT);
    rows = $state<Row[]>([]);
    next = $state<string | null>(null);
    total = $state<number | undefined>(undefined);
    loading = $state(false);
    loaded = $state(false);
    serverError = $state<string | undefined>(undefined);
    outside = $state(0);
    checked = $state<Record<string, number>>({});
    anchor = $state<string | undefined>(undefined);
    ready = $state(false);
    searches = $state(0);
    scrollTop = 0;
    private run = 0;
    private held: Qso[] = [];
    private readonly compiled = $derived(this.ready ? matcher(this.q) : undefined);

    constructor(private readonly logbook: LogbookState) {
        loadQuery().then(() => {
            this.ready = true;
            this.release();
        });
        logbook.onQso((qso) => {
            if (!this.loaded && !this.loading) return;
            if (!this.ready || this.loading) this.held.push(qso);
            else this.receive(qso);
        });
        logbook.onReset(() => {
            if (this.loaded || this.loading) void this.search();
        });
    }

    get href(): string {
        const params = new URLSearchParams();
        if (this.q !== '') params.set('q', this.q);
        if (this.sort !== DEFAULT_SORT) params.set('sort', this.sort);
        const search = params.toString();
        return `/logbooks/${this.logbook.logbook.id}${search === '' ? '' : `?${search}`}`;
    }

    show(q: string, sort: Sort) {
        if (this.loaded && this.serverError === undefined && q === this.q && sort === this.sort) {
            return;
        }
        this.q = q;
        this.sort = sort;
        void this.search();
    }

    async search() {
        const mine = ++this.run;
        const { q, sort } = this;
        this.loading = true;
        try {
            const found = await api.searchQsos(this.logbook.logbook.id, { q, sort, limit: PAGE });
            if (mine !== this.run) return;
            this.rows = found.qsos.map((qso) => ({ qso, stale: false }));
            this.next = found.next;
            this.total = found.total;
            this.outside = 0;
            this.serverError = undefined;
            this.loaded = true;
            this.scrollTop = 0;
            this.searches += 1;
        } catch (e) {
            if (mine !== this.run) return;
            this.serverError = e instanceof Error ? e.message : String(e);
        } finally {
            if (mine === this.run) {
                this.loading = false;
                this.release();
            }
        }
    }

    async more(): Promise<string | undefined> {
        if (this.next === null || this.loading) return;
        const mine = this.run;
        this.loading = true;
        try {
            const found = await api.searchQsos(this.logbook.logbook.id, {
                q: this.q,
                sort: this.sort,
                cursor: this.next,
                limit: PAGE
            });
            if (mine !== this.run) return;
            const known = new Set(this.rows.map((r) => r.qso.id));
            this.rows = [
                ...this.rows,
                ...found.qsos
                    .filter((qso) => !known.has(qso.id))
                    .map((qso) => ({ qso, stale: false }))
            ];
            this.next = found.next;
        } catch (e) {
            return e instanceof Error ? e.message : String(e);
        } finally {
            if (mine === this.run) {
                this.loading = false;
                this.release();
            }
        }
    }

    private receive(qso: Qso) {
        const matches = this.logbook.matches(qso, this.compiled) ?? false;
        const applied = applyEvent(
            this.rows,
            qso,
            matches,
            this.next === null,
            comparator(this.sort)
        );
        if (applied.kind === 'outside') this.outside += 1;
        if ('rows' in applied) this.rows = applied.rows;
        if (applied.kind === 'inserted' && qso.revision === 1 && this.total !== undefined) {
            this.total += 1;
        }
    }

    private release() {
        if (!this.ready || this.loading) return;
        const queued = this.held;
        this.held = [];
        queued.forEach((qso) => this.receive(qso));
    }

    toggle(qso: Qso, range: boolean) {
        const next = { ...this.checked };
        const ids = this.rows.map((r) => r.qso.id);
        const a = this.anchor === undefined ? -1 : ids.indexOf(this.anchor);
        const b = ids.indexOf(qso.id);
        if (range && a >= 0 && b >= 0) {
            for (let i = Math.min(a, b); i <= Math.max(a, b); i++) {
                next[ids[i]] = this.rows[i].qso.revision;
            }
        } else if (qso.id in next) {
            delete next[qso.id];
        } else {
            next[qso.id] = qso.revision;
        }
        this.checked = next;
        this.anchor = qso.id;
    }

    get allLoadedChecked(): boolean {
        return this.rows.length > 0 && this.rows.every((r) => r.qso.id in this.checked);
    }

    toggleAll() {
        if (this.allLoadedChecked) {
            const next = { ...this.checked };
            this.rows.forEach((r) => delete next[r.qso.id]);
            this.checked = next;
        } else {
            this.checked = {
                ...this.checked,
                ...Object.fromEntries(this.rows.map((r) => [r.qso.id, r.qso.revision]))
            };
        }
    }

    clearChecked() {
        this.checked = {};
        this.anchor = undefined;
    }

    applied(updated: Qso[]) {
        this.rows = replaceRows(
            this.rows,
            updated,
            (qso) => this.logbook.matches(qso, this.compiled) ?? true
        );
        const next = { ...this.checked };
        for (const qso of updated) if (qso.id in next) next[qso.id] = qso.revision;
        this.checked = next;
    }

    removed(ids: Set<string>) {
        this.rows = this.rows.filter((r) => !ids.has(r.qso.id));
        if (this.total !== undefined) this.total -= ids.size;
        const left = Object.entries(this.checked).filter(([id]) => !ids.has(id));
        if (left.length === 0) this.clearChecked();
        else this.checked = Object.fromEntries(left);
    }
}
