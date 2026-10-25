<script lang="ts">
    import { onMount, untrack } from 'svelte';
    import { MediaQuery } from 'svelte/reactivity';
    import { goto } from '$app/navigation';
    import { page } from '$app/state';
    import * as api from '#lib/api/api.ts';
    import { ApiError } from '#lib/api/client.ts';
    import type { Fields, Qso, Target } from '#lib/api/types.ts';
    import PageHeader from '#lib/components/PageHeader.svelte';
    import PageTitle from '#lib/components/PageTitle.svelte';
    import BulkEdit from '#lib/components/log/BulkEdit.svelte';
    import ColumnsPanel from '#lib/components/log/ColumnsPanel.svelte';
    import QsoDetail from '#lib/components/log/QsoDetail.svelte';
    import QueryBar from '#lib/components/log/QueryBar.svelte';
    import Button from '#lib/components/ui/Button.svelte';
    import ConfirmDialog from '#lib/components/ui/ConfirmDialog.svelte';
    import Icon from '#lib/components/ui/Icon.svelte';
    import QslBadge from '#lib/components/ui/QslBadge.svelte';
    import { adifFieldOf, column, type Column } from '#lib/log/columns.ts';
    import { applyEvent, replaceRows, undoOf, type Row } from '#lib/log/rows.ts';
    import { loadColumns, saveColumns } from '#lib/log/settings.ts';
    import { m } from '#lib/paraglide/messages.js';
    import { adifDate, hhmm, isoDate, parks, rst, utcClock } from '#lib/qso/format.ts';
    import { loadQuery, matcher } from '#lib/query/index.ts';
    import { useLogbook } from '#lib/state/logbook.svelte.ts';
    import { toasts } from '#lib/state/toast.svelte.ts';

    const PAGE = 100;

    const logbook = useLogbook();
    const logbookId = logbook.logbook.id;
    const desktop = new MediaQuery('min-width: 768px');
    const subtitle = $derived(
        `${logbook.logbook.name} · ${m.common_qso_count({ count: logbook.logbook.last_seq })}`
    );

    let ready = $state(false);
    let rows = $state<Row[]>([]);
    let next = $state<string | null>(null);
    let total = $state<number | undefined>(undefined);
    let loading = $state(false);
    let loaded = $state(false);
    let serverError = $state<string | undefined>(undefined);
    let outside = $state(0);
    let checked = $state<Record<string, number>>({});
    let anchor = $state<string | undefined>(undefined);
    let bulkOpen = $state(false);
    let busy = $state(false);
    let confirmDelete = $state(false);
    let fetched = $state<Qso | undefined>(undefined);
    let scroller = $state<HTMLElement>();
    let sentinel = $state<HTMLElement>();
    let columns = $state(loadColumns());
    let run = 0;

    const q = $derived(page.url.searchParams.get('q') ?? '');
    const openId = $derived(page.url.searchParams.get('qso'));
    const compiled = $derived(ready ? matcher(q) : undefined);
    const checkedIds = $derived(Object.keys(checked));
    const n = $derived(checkedIds.length);
    const allLoadedChecked = $derived(rows.length > 0 && rows.every((r) => r.qso.id in checked));
    const openRow = $derived(rows.find((r) => r.qso.id === openId));
    const openQso = $derived(openRow?.qso ?? (fetched?.id === openId ? fetched : undefined));
    const today = $derived(utcClock(new Date()).date);
    const showBulk = $derived(bulkOpen && n > 0 && desktop.current);
    const shown = $derived(columns.map(column).filter((c): c is Column => c !== undefined));

    onMount(() => {
        loadQuery().then(() => (ready = true));
        const offQso = logbook.onQso((qso) => {
            const matches = logbook.matches(qso, compiled) ?? false;
            const applied = applyEvent(rows, qso, matches, next === null);
            if (applied.kind === 'outside') outside += 1;
            if ('rows' in applied) rows = applied.rows;
            if (applied.kind === 'inserted' && qso.revision === 1 && total !== undefined)
                total += 1;
        });
        const offReset = logbook.onReset(() => search(q));
        return () => {
            offQso();
            offReset();
        };
    });

    $effect(() => {
        toasts.lifted = n > 0 && desktop.current;
        return () => (toasts.lifted = false);
    });

    $effect(() => {
        const query = q;
        untrack(() => search(query));
    });

    $effect(() => {
        const id = openId;
        if (id === null || rows.some((r) => r.qso.id === id) || fetched?.id === id) return;
        api.getQso(logbookId, id)
            .then((qso) => (fetched = qso))
            .catch(() => replaceUrl({ qso: null }));
    });

    $effect(() => {
        if (!sentinel) return;
        const observer = new IntersectionObserver(
            (entries) => {
                if (entries.some((e) => e.isIntersecting)) more();
            },
            { root: desktop.current ? scroller : null, rootMargin: '400px' }
        );
        observer.observe(sentinel);
        return () => observer.disconnect();
    });

    function replaceUrl(change: { q?: string; qso?: string | null }) {
        const url = new URL(page.url.href);
        if (change.q !== undefined) {
            if (change.q === '') url.searchParams.delete('q');
            else url.searchParams.set('q', change.q);
        }
        if (change.qso !== undefined) {
            if (change.qso === null) url.searchParams.delete('qso');
            else url.searchParams.set('qso', change.qso);
        }
        goto(url, { replace: true, reset: false });
    }

    async function search(query: string) {
        const mine = ++run;
        loading = true;
        try {
            const found = await api.searchQsos(logbookId, { q: query, limit: PAGE });
            if (mine !== run) return;
            rows = found.qsos.map((qso) => ({ qso, stale: false }));
            next = found.next;
            total = found.total;
            outside = 0;
            serverError = undefined;
            loaded = true;
            scroller?.scrollTo({ top: 0 });
        } catch (e) {
            if (mine !== run) return;
            serverError = e instanceof Error ? e.message : String(e);
        } finally {
            if (mine === run) loading = false;
        }
    }

    async function more() {
        if (next === null || loading) return;
        const mine = run;
        loading = true;
        try {
            const found = await api.searchQsos(logbookId, { q, cursor: next, limit: PAGE });
            if (mine !== run) return;
            const known = new Set(rows.map((r) => r.qso.id));
            rows = [
                ...rows,
                ...found.qsos
                    .filter((qso) => !known.has(qso.id))
                    .map((qso) => ({ qso, stale: false }))
            ];
            next = found.next;
        } catch (e) {
            toasts.error(e instanceof Error ? e.message : String(e));
        } finally {
            if (mine === run) loading = false;
        }
    }

    function toggle(event: MouseEvent, qso: Qso) {
        event.stopPropagation();
        const next = { ...checked };
        const ids = rows.map((r) => r.qso.id);
        const a = anchor === undefined ? -1 : ids.indexOf(anchor);
        const b = ids.indexOf(qso.id);
        if (event.shiftKey && a >= 0 && b >= 0) {
            for (let i = Math.min(a, b); i <= Math.max(a, b); i++) {
                next[ids[i]] = rows[i].qso.revision;
            }
        } else if (qso.id in next) {
            delete next[qso.id];
        } else {
            next[qso.id] = qso.revision;
        }
        checked = next;
        anchor = qso.id;
    }

    function toggleAll() {
        if (allLoadedChecked) {
            const next = { ...checked };
            rows.forEach((r) => delete next[r.qso.id]);
            checked = next;
        } else {
            checked = {
                ...checked,
                ...Object.fromEntries(rows.map((r) => [r.qso.id, r.qso.revision]))
            };
        }
    }

    async function checkAllMatching() {
        try {
            const { qsos } = await api.targets(logbookId, q);
            checked = Object.fromEntries(qsos.map((t) => [t.id, t.revision]));
        } catch (e) {
            toasts.error(e instanceof Error ? e.message : String(e));
        }
    }

    function clearChecked() {
        checked = {};
        anchor = undefined;
        bulkOpen = false;
    }

    function open(qso: Qso) {
        if (desktop.current) replaceUrl({ qso: openId === qso.id ? null : qso.id });
        else goto(`/logbooks/${logbookId}/qsos/${qso.id}`);
    }

    function applied(updated: Qso[]) {
        rows = replaceRows(rows, updated, (qso) => logbook.matches(qso, compiled) ?? true);
        const next = { ...checked };
        for (const qso of updated) if (qso.id in next) next[qso.id] = qso.revision;
        checked = next;
    }

    async function changeQsl(targets: Target[], fields: Fields, done: (count: number) => string) {
        const before = rows.filter((r) => checked[r.qso.id] !== undefined).map((r) => r.qso);
        busy = true;
        try {
            const outcome = await api.updateQsl(logbookId, targets, fields);
            applied(outcome.updated);
            const undo = undoOf(before, outcome.updated, Object.keys(fields));
            const skipped = outcome.skipped.length;
            const text = `${done(outcome.updated.length)}${
                skipped > 0 ? m.log_skipped_meanwhile({ count: skipped }) : ''
            }`;
            toasts.show(text, {
                action:
                    undo === undefined
                        ? undefined
                        : {
                              label: m.log_undo(),
                              run: async () => {
                                  try {
                                      for (const group of undo) {
                                          const back = await api.updateQsl(
                                              logbookId,
                                              group.qsos,
                                              group.fields
                                          );
                                          applied(back.updated);
                                      }
                                  } catch (e) {
                                      toasts.error(e instanceof Error ? e.message : String(e));
                                  }
                              }
                          }
            });
        } catch (e) {
            toasts.error(e instanceof Error ? e.message : String(e));
        } finally {
            busy = false;
        }
    }

    const targetsOfChecked = (): Target[] =>
        Object.entries(checked).map(([id, revision]) => ({ id, revision }));

    async function deleteChecked() {
        busy = true;
        let deleted = 0;
        let failed = 0;
        for (const target of targetsOfChecked()) {
            try {
                await api.deleteQso(logbookId, target.id, target.revision);
                deleted += 1;
            } catch (e) {
                failed += 1;
                if (!(e instanceof ApiError) || e.retryable) break;
            }
        }
        const gone = new Set(Object.keys(checked));
        rows = rows.filter((r) => !gone.has(r.qso.id) || r.stale);
        if (total !== undefined) total -= deleted;
        clearChecked();
        busy = false;
        toasts.show(
            `${m.log_deleted({ count: deleted })}${failed > 0 ? m.log_delete_failed({ count: failed }) : ''}`
        );
    }

    function changeColumns(next: string[]) {
        columns = next;
        saveColumns(next);
    }

    const checkHalo =
        '-m-2 inline-flex size-8 items-center justify-center rounded-full align-middle group-hover/check:bg-surface-2';

    const stationOf = (qso: Qso) =>
        [qso.fields.STATION_CALLSIGN, logbook.stationName(qso.station_id)]
            .filter((s) => s)
            .join(' · ');

    const groups = $derived.by(() => {
        const out: { key: string; date: string; where: string; rows: Row[] }[] = [];
        for (const row of rows) {
            const date = isoDate(row.qso.fields.QSO_DATE);
            const where = [row.qso.fields.STATION_CALLSIGN, parks(row.qso.fields)[0]]
                .filter((s) => s)
                .join(' · ');
            const last = out.at(-1);
            if (last && last.date === date && last.where === where) last.rows.push(row);
            else out.push({ key: `${date}|${where}|${row.qso.id}`, date, where, rows: [row] });
        }
        return out;
    });

    const matchText = $derived(total === undefined ? '' : m.log_matching({ count: total }));
</script>

<PageTitle title={m.nav_logbook()} />

{#snippet counts()}
    {#if outside > 0}
        <button type="button" class="text-xs text-info" onclick={() => search(q)}
            >{m.log_new_outside({ count: outside })}</button
        >
    {/if}
    <span class="text-[13px] text-muted">{matchText}</span>
{/snippet}

{#snippet qsl(sent: string | undefined, rcvd: string | undefined)}
    <td class="px-3">
        <QslBadge value={sent} />
        <span class="ml-1"><QslBadge value={rcvd} /></span>
    </td>
{/snippet}

{#snippet cell(id: string, qso: Qso)}
    {@const f = qso.fields}
    {#if id === 'time'}
        <td class="px-3 font-mono"
            ><span class="text-muted">{isoDate(f.QSO_DATE)}</span> {hhmm(f.TIME_ON)}</td
        >
    {:else if id === 'call'}
        <td class="px-3 font-mono text-sm font-semibold">{f.CALL}</td>
    {:else if id === 'qsl'}
        {@render qsl(f.QSL_SENT, f.QSL_RCVD)}
    {:else if id === 'lotw'}
        {@render qsl(f.LOTW_QSL_SENT, f.LOTW_QSL_RCVD)}
    {:else if id === 'eqsl'}
        {@render qsl(f.EQSL_QSL_SENT, f.EQSL_QSL_RCVD)}
    {:else if id === 'band'}
        <td class="px-3 font-mono">{f.BAND}</td>
    {:else if id === 'freq'}
        <td class={['px-3 text-right font-mono', !f.FREQ && 'text-muted']}>{f.FREQ || '—'}</td>
    {:else if id === 'mode'}
        <td class="px-3 font-mono">
            {f.MODE}
            {#if f.SUBMODE}<span class="text-muted">{f.SUBMODE}</span>{/if}
        </td>
    {:else if id === 'rst'}
        <td class="px-3 font-mono text-muted">{rst(f)}</td>
    {:else if id === 'txpwr'}
        <td class="px-3 text-right font-mono">{f.TX_PWR ?? ''}</td>
    {:else if id === 'contest'}
        <td class="px-3 font-mono">{f.CONTEST_ID ?? ''}</td>
    {:else if id === 'number'}
        <td class="px-3 font-mono text-muted"
            >{f.STX_STRING || f.SRX_STRING
                ? `${f.STX_STRING || '—'} / ${f.SRX_STRING || '—'}`
                : ''}</td
        >
    {:else if id === 'station'}
        <td class="px-3 text-muted">{stationOf(qso)}</td>
    {:else if id === 'instrument'}
        <td class="px-3 text-muted"
            >{logbook.instrumentName(qso.instrument_id) ?? f.MY_RIG ?? ''}</td
        >
    {:else if id === 'park'}
        <td class="px-3 font-mono text-info">{parks(f).join(', ')}</td>
    {:else}
        {@const value = f[adifFieldOf(id) ?? ''] ?? ''}
        <td class="px-3"><div class="max-w-[24rem] truncate" title={value}>{value}</div></td>
    {/if}
{/snippet}

{#snippet status()}
    <div
        bind:this={sentinel}
        role="status"
        aria-live="polite"
        class={[
            'sticky left-0 flex items-center justify-center gap-2 px-6 py-4 text-[13px] text-muted',
            n > 0 && desktop.current && 'pb-24'
        ]}
    >
        {#if loading}
            <Icon name="spinner" size={14} strokeWidth={2.5} />
            <span>{rows.length === 0 ? m.log_loading_qsos() : m.log_loading_older()}</span>
        {:else if loaded && rows.length === 0}
            <span>{m.log_no_match()}</span>
        {:else if loaded && next === null && rows.length > 0}
            <span>{m.log_all_loaded()}</span>
        {/if}
    </div>
{/snippet}

{#if desktop.current}
    <div class="flex h-dvh min-w-0 flex-col">
        <PageHeader title={m.nav_logbook()} {subtitle}>
            {#snippet actions()}
                <ColumnsPanel {columns} onchange={changeColumns} />
                <Button href={`/logbooks/${logbookId}/import-export`}>{m.log_export_adif()}</Button>
                <Button variant="primary" href={`/logbooks/${logbookId}/entry`}
                    >{m.log_new_qso()}</Button
                >
            {/snippet}
            {#snippet toolbar()}
                <QueryBar
                    query={q}
                    {ready}
                    {serverError}
                    onsearch={(query) => replaceUrl({ q: query })}
                    aside={counts}
                />
            {/snippet}
        </PageHeader>

        <main class="relative flex min-h-0 flex-1">
            <div bind:this={scroller} class="min-w-0 flex-1 overflow-auto">
                <table class="w-full border-collapse text-[13px]">
                    <thead class="sticky top-0 z-[1] bg-bg">
                        <tr
                            class="text-left text-xs text-muted [&>th:last-child]:pr-6 [&>th:not(:first-child)]:border-l [&>th:not(:first-child)]:border-l-surface-2"
                        >
                            <th
                                scope="col"
                                class="group/check w-14 cursor-pointer border-b border-border py-2 pr-3 pl-6"
                                onclick={toggleAll}
                            >
                                <span class={checkHalo}>
                                    <input
                                        type="checkbox"
                                        aria-label={m.log_check_all_loaded()}
                                        checked={allLoadedChecked}
                                        onclick={(e) => {
                                            e.stopPropagation();
                                            toggleAll();
                                        }}
                                        class="size-4 cursor-pointer accent-info"
                                    />
                                </span>
                            </th>
                            {#each shown as c (c.id)}
                                <th
                                    scope="col"
                                    class={[
                                        'border-b border-border px-3 py-2 font-medium whitespace-nowrap',
                                        c.right && 'text-right'
                                    ]}>{c.label}</th
                                >
                            {/each}
                            <th aria-hidden="true" class="w-full border-b border-border"></th>
                        </tr>
                    </thead>
                    <tbody>
                        {#each rows as row (row.qso.id)}
                            {@const f = row.qso.fields}
                            {@const isChecked = row.qso.id in checked}
                            {@const isOpen = row.qso.id === openId}
                            <tr
                                aria-current={isOpen ? 'true' : undefined}
                                title={row.stale ? m.log_row_stale() : undefined}
                                class={[
                                    'cursor-pointer [&>td]:border-b [&>td]:border-surface-2 [&>td]:py-[7px] [&>td]:whitespace-nowrap [&>td:last-child]:pr-6 [&>td:not(:first-child)]:border-l',
                                    isChecked
                                        ? 'bg-selected'
                                        : isOpen
                                          ? 'bg-surface'
                                          : 'even:bg-zebra hover:bg-surface/60',
                                    isOpen && 'shadow-[inset_3px_0_0_var(--accent)]',
                                    row.stale && 'opacity-50'
                                ]}
                                onclick={() => open(row.qso)}
                            >
                                <td
                                    class="group/check pr-3 pl-6"
                                    onclick={(e) => toggle(e, row.qso)}
                                    onmousedown={(e) => {
                                        if (e.shiftKey) e.preventDefault();
                                    }}
                                >
                                    <span class={checkHalo}>
                                        <input
                                            type="checkbox"
                                            aria-label={m.log_check_row({
                                                call: f.CALL ?? '',
                                                date: isoDate(f.QSO_DATE),
                                                time: hhmm(f.TIME_ON)
                                            })}
                                            checked={isChecked}
                                            onclick={(e) => toggle(e, row.qso)}
                                            class="size-4 cursor-pointer accent-info"
                                        />
                                    </span>
                                </td>
                                {#each shown as c (c.id)}
                                    {@render cell(c.id, row.qso)}
                                {/each}
                                <td aria-hidden="true"></td>
                            </tr>
                        {/each}
                    </tbody>
                </table>
                {@render status()}
            </div>

            {#if n > 0}
                <div
                    role="toolbar"
                    aria-label={m.log_checked_qsos()}
                    class={[
                        'absolute bottom-4 left-6 z-[4] flex flex-wrap items-center gap-2 rounded-[10px] border border-info-strong bg-selected px-2.5 py-2 text-[13px] shadow-lg shadow-shadow',
                        showBulk || openQso ? 'right-[404px]' : 'right-6'
                    ]}
                >
                    <span class="pl-1 font-semibold">{m.log_checked_count({ count: n })}</span>
                    {#if total !== undefined && n < total}
                        <button
                            type="button"
                            class="h-[30px] px-2 text-info"
                            onclick={checkAllMatching}
                            >{m.log_check_all_matching({ count: total })}</button
                        >
                    {/if}
                    <button type="button" class="h-[30px] px-2 text-info" onclick={clearChecked}
                        >{m.common_clear()}</button
                    >
                    <span aria-hidden="true" class="mx-1 h-5 w-px bg-border"></span>
                    <Button
                        size="sm"
                        disabled={busy}
                        onclick={() =>
                            changeQsl(
                                targetsOfChecked(),
                                { QSL_SENT: 'Y', QSLSDATE: adifDate(today) },
                                (count) => m.log_marked_sent({ count })
                            )}>{m.log_mark_sent_today()}</Button
                    >
                    <Button
                        size="sm"
                        disabled={busy}
                        onclick={() =>
                            changeQsl(
                                targetsOfChecked(),
                                { QSL_RCVD: 'Y', QSLRDATE: adifDate(today) },
                                (count) => m.log_marked_received({ count })
                            )}>{m.log_mark_received()}</Button
                    >
                    <Button
                        size="sm"
                        variant="toggle"
                        pressed={showBulk}
                        onclick={() => (bulkOpen = !bulkOpen)}>{m.log_edit_fields()}</Button
                    >
                    <Button
                        size="sm"
                        variant="danger"
                        disabled={busy}
                        onclick={() => (confirmDelete = true)}>{m.common_delete()}</Button
                    >
                </div>
            {/if}

            {#if showBulk}
                <BulkEdit
                    count={n}
                    known={rows.filter((r) => r.qso.id in checked).map((r) => r.qso)}
                    {today}
                    {busy}
                    onapply={(fields) =>
                        changeQsl(targetsOfChecked(), fields, (count) =>
                            m.log_updated_qsl({ count })
                        )}
                    onclose={() => (bulkOpen = false)}
                />
            {:else if openQso}
                <QsoDetail
                    qso={openQso}
                    stale={openRow?.stale}
                    onclose={() => replaceUrl({ qso: null })}
                />
            {/if}
        </main>
    </div>
{:else}
    <PageHeader title={m.nav_logbook()} {subtitle}>
        {#snippet toolbar()}
            <div class="flex flex-col gap-2.5">
                <QueryBar
                    query={q}
                    {ready}
                    {serverError}
                    compact
                    onsearch={(query) => replaceUrl({ q: query })}
                />
                <div class="flex items-center justify-between">
                    {@render counts()}
                </div>
            </div>
        {/snippet}
    </PageHeader>
    <main class="flex flex-col">
        {#each groups as group (group.key)}
            <div class="flex items-center justify-between px-4 pt-2.5 pb-1 text-xs text-muted">
                <span class="font-mono">{group.date}</span>
                <span>{group.where}</span>
            </div>
            {#each group.rows as row (row.qso.id)}
                {@const f = row.qso.fields}
                <a
                    href={`/logbooks/${logbookId}/qsos/${row.qso.id}`}
                    class={[
                        'flex min-h-[60px] items-center gap-3 border-b border-surface-2 px-4 py-2 text-text no-underline',
                        row.stale && 'opacity-50'
                    ]}
                >
                    <span class="w-11 font-mono text-[13px] text-muted">{hhmm(f.TIME_ON)}</span>
                    <span class="flex min-w-0 flex-1 flex-col gap-0.5">
                        <span class="font-mono text-[17px] font-semibold tracking-wide"
                            >{f.CALL}</span
                        >
                        <span class="truncate text-xs text-muted"
                            >{[f.NAME, f.QTH, f.COMMENT].filter((s) => s).join(' · ')}</span
                        >
                    </span>
                    <span class="flex flex-col items-end gap-1">
                        <span class="font-mono text-[13px]">{f.BAND} {f.MODE}</span>
                        <span class="flex gap-1">
                            <QslBadge value={f.QSL_SENT} small />
                            <QslBadge value={f.QSL_RCVD} small />
                        </span>
                    </span>
                </a>
            {/each}
        {/each}
        {@render status()}
    </main>
{/if}

<ConfirmDialog
    bind:open={confirmDelete}
    title={m.log_delete_title({ count: n })}
    description={m.log_delete_description()}
    confirm={m.common_delete()}
    danger
    onconfirm={deleteChecked}
/>
