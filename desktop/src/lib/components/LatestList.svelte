<script lang="ts">
    import type { Qso } from '#client/api/types.ts';
    import type { Outgoing } from '#client/entry/outbox.ts';
    import { compareNewestFirst, hhmm, isoDate } from '#client/qso/format.ts';
    import { outbox } from '#lib/entry/outbox.svelte.ts';
    import { m } from '#lib/paraglide/messages.js';

    let {
        unsent,
        recent,
        contest,
        onsendnow
    }: {
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

    const headers = $derived([
        m.latest_utc(),
        m.latest_call(),
        m.latest_band_mode(),
        m.latest_rst(),
        ...(contest ? [m.latest_number()] : [])
    ]);
</script>

<section aria-label={m.latest_title()} class="flex min-h-0 flex-1 flex-col gap-1.5">
    {#if unsent.length > 0}
        <div
            role="status"
            class="flex flex-none flex-col gap-1 rounded-lg border border-warn-border bg-warn-surface px-2.5 py-1.5 text-xs"
        >
            <div class="flex items-center gap-2.5">
                <svg
                    aria-hidden="true"
                    width="14"
                    height="14"
                    viewBox="0 0 24 24"
                    fill="none"
                    stroke="currentColor"
                    stroke-width="2"
                    stroke-linecap="round"
                    class="flex-none text-warn"
                    ><path
                        d="M3 3l18 18M8.5 8.6A5 5 0 0 0 6 18h11M20 15.8A4 4 0 0 0 17 9h-1.3A6 6 0 0 0 10.9 6"
                    /></svg
                >
                <span class="flex-1"
                    ><strong class="font-semibold text-warn"
                        >{m.latest_unsent({ count: unsent.length })}</strong
                    >
                    <span class="text-muted">{m.latest_kept_here()}</span></span
                >
                <button
                    type="button"
                    disabled={outbox.sending}
                    class="h-7 rounded-md border border-warn px-2.5 font-semibold text-warn"
                    onclick={onsendnow}
                    >{outbox.sending ? m.latest_sending() : m.latest_send_now()}</button
                >
            </div>
            {#each unsent.filter((i) => i.error) as item (item.id)}
                <div class="flex items-center gap-2 text-2xs">
                    <span class="font-mono font-semibold">{item.fields.CALL}</span>
                    <span class="min-w-0 flex-1 truncate text-muted select-text">{item.error}</span>
                    <button
                        type="button"
                        class="text-error-text"
                        onclick={() => outbox.discard(item.id)}>{m.latest_discard()}</button
                    >
                </div>
            {/each}
        </div>
    {/if}
    <h2 class="flex-none eyebrow text-2xs">{m.latest_title()}</h2>
    <div class="min-h-0 flex-1 overflow-y-auto rounded-lg border border-border bg-surface">
        <table class="w-full border-collapse text-sm">
            <thead>
                <tr class="text-left text-2xs text-muted">
                    {#each headers as header (header)}
                        <th
                            scope="col"
                            class="sticky top-0 border-b border-border bg-surface px-2.5 py-1.5 font-medium"
                            >{header}</th
                        >
                    {/each}
                    <th
                        scope="col"
                        class="sticky top-0 border-b border-border bg-surface px-2.5 py-1.5"
                        ><span class="sr-only">{m.latest_sync()}</span></th
                    >
                </tr>
            </thead>
            <tbody class="select-text">
                {#each rows as row (row.id)}
                    {@const f = row.fields}
                    <tr
                        class="align-top [&>td]:border-b [&>td]:border-surface-2 [&>td]:px-2.5 [&>td]:py-1"
                    >
                        <td class="font-mono whitespace-nowrap"
                            ><span class="text-muted">{isoDate(f.QSO_DATE).slice(5)}</span>
                            {hhmm(f.TIME_ON)}</td
                        >
                        <td class="font-mono text-base font-semibold">{f.CALL}</td>
                        <td class="font-mono whitespace-nowrap"
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
                                title={row.unsent ? m.latest_unsent_dot() : m.latest_synced_dot()}
                                class={[
                                    'inline-block size-[8px] rounded-full',
                                    row.unsent ? 'bg-warn' : 'bg-ok'
                                ]}
                            ></span>
                        </td>
                    </tr>
                {:else}
                    <tr
                        ><td colspan="6" class="px-2.5 py-5 text-center text-muted"
                            >{m.latest_empty()}</td
                        ></tr
                    >
                {/each}
            </tbody>
        </table>
    </div>
</section>
