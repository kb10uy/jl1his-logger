<script lang="ts">
    import type { Qso } from '#lib/api/types.ts';
    import Icon from '#lib/components/ui/Icon.svelte';
    import type { Outgoing } from '#lib/entry/outbox.ts';
    import { compareNewestFirst, hhmm, isoDate } from '#lib/qso/format.ts';
    import { outbox } from '#lib/state/outbox.svelte.ts';

    let {
        logbookId,
        stationName,
        unsent,
        recent,
        contest,
        onsendnow
    }: {
        logbookId: string;
        stationName: string;
        unsent: Outgoing[];
        recent: Qso[];
        contest: boolean;
        onsendnow: () => void;
    } = $props();

    const rows = $derived(
        [
            ...unsent
                .filter((i) => !recent.some((q) => q.id === i.id))
                .map((i) => ({ id: i.id, fields: i.fields, unsent: true })),
            ...recent.map((q) => ({ id: q.id, fields: q.fields, unsent: false }))
        ].sort(compareNewestFirst)
    );

    const pair = (a: string | undefined, b: string | undefined) => `${a || '—'} / ${b || '—'}`;
</script>

<aside
    aria-label="Latest QSOs at this Station"
    class="flex min-w-0 flex-[1_1_440px] flex-col gap-3"
>
    {#if unsent.length > 0}
        <section
            aria-label="Unsent"
            class="flex flex-col gap-2 rounded-lg border border-warn-border bg-warn-surface px-4 py-3 text-[13px]"
        >
            <div class="flex items-center gap-3">
                <Icon name="offline" size={16} class="text-warn" />
                <span class="flex-1"
                    ><strong class="font-semibold text-warn">{unsent.length} unsent</strong>
                    <span class="text-muted">· kept on this device</span></span
                >
                <button
                    type="button"
                    disabled={outbox.sending}
                    class="h-8 rounded-md border border-warn px-3 font-semibold text-warn"
                    onclick={onsendnow}>{outbox.sending ? 'Sending…' : 'Send now'}</button
                >
            </div>
            {#each unsent.filter((i) => i.error) as item (item.id)}
                <div class="flex items-center gap-2 text-xs">
                    <span class="font-mono font-semibold">{item.fields.CALL}</span>
                    <span class="flex-1 text-muted">{item.error}</span>
                    <button
                        type="button"
                        class="text-error-text"
                        onclick={() => outbox.discard(item.id)}>Discard</button
                    >
                </div>
            {/each}
        </section>
    {/if}
    <div class="flex items-baseline justify-between gap-2">
        <h2 class="eyebrow">Latest at this Station</h2>
        <a
            href={`/logbooks/${logbookId}/log?q=${encodeURIComponent(`station:"${stationName}"`)}`}
            class="text-[13px] no-underline">Open in log</a
        >
    </div>
    <div class="overflow-x-auto rounded-lg border border-border bg-surface">
        <table class="w-full border-collapse text-[13px]">
            <thead>
                <tr class="text-left text-xs text-muted">
                    {#each ['UTC', 'Call', 'Band / Mode', 'RST'] as h (h)}
                        <th
                            scope="col"
                            class={[
                                'border-b border-border px-3 py-2 font-medium',
                                h === 'Band / Mode' && 'max-md:hidden'
                            ]}>{h}</th
                        >
                    {/each}
                    {#if contest}
                        <th scope="col" class="border-b border-border px-3 py-2 font-medium"
                            >Number S / R</th
                        >
                    {/if}
                    <th scope="col" class="border-b border-border px-3 py-2"
                        ><span class="sr-only">Sync</span></th
                    >
                </tr>
            </thead>
            <tbody>
                {#each rows as row (row.id)}
                    {@const f = row.fields}
                    <tr
                        class="align-top [&>td]:border-b [&>td]:border-surface-2 [&>td]:px-3 [&>td]:py-2"
                    >
                        <td class="font-mono whitespace-nowrap"
                            ><span class="text-muted">{isoDate(f.QSO_DATE).slice(5)}</span>
                            {hhmm(f.TIME_ON)}</td
                        >
                        <td class="font-mono text-sm font-semibold">{f.CALL}</td>
                        <td class="font-mono whitespace-nowrap max-md:hidden"
                            >{f.BAND}
                            {f.MODE} <span class="text-muted">{f.SUBMODE ?? ''}</span></td
                        >
                        <td class="font-mono whitespace-nowrap text-muted"
                            >{pair(f.RST_SENT, f.RST_RCVD)}</td
                        >
                        {#if contest}
                            <td class="font-mono whitespace-nowrap"
                                >{pair(f.STX_STRING, f.SRX_STRING)}</td
                            >
                        {/if}
                        <td class="text-right">
                            <span
                                title={row.unsent ? 'unsent' : 'synced'}
                                class={[
                                    'inline-block size-2 rounded-full',
                                    row.unsent ? 'bg-warn' : 'bg-ok'
                                ]}
                            ></span>
                        </td>
                    </tr>
                {:else}
                    <tr
                        ><td colspan="6" class="px-3 py-6 text-center text-muted"
                            >No QSOs at this Station yet.</td
                        ></tr
                    >
                {/each}
            </tbody>
        </table>
    </div>
</aside>
