<script lang="ts">
    import type { Batch } from '#lib/api/types.ts';
    import { KIND_LABELS, recordOnly } from '#lib/transfer/transfer.ts';

    let {
        batches,
        callsigns,
        results = {},
        applying,
        onapply
    }: {
        batches: Batch[];
        callsigns: Record<string, string>;
        results?: Record<string, string>;
        applying?: string;
        onapply?: (batch: Batch) => void;
    } = $props();

    const when = (at: string) => at.replace('T', ' ').slice(0, 16);
</script>

<div class="overflow-x-auto rounded-lg border border-border bg-surface">
    <table class="w-full border-collapse text-[13px]">
        <thead>
            <tr class="text-left text-xs text-muted">
                <th scope="col" class="border-b border-border px-3 py-2 font-medium">Kind</th>
                <th scope="col" class="border-b border-border px-3 py-2 font-medium">Created</th>
                <th scope="col" class="border-b border-border px-3 py-2 text-right font-medium"
                    >QSOs</th
                >
                {#if onapply}
                    <th scope="col" class="border-b border-border px-3 py-2 font-medium">State</th>
                {/if}
            </tr>
        </thead>
        <tbody>
            {#each batches as batch (batch.id)}
                <tr class="border-b border-surface-2 last:border-b-0">
                    <td class="px-3 py-2.5 whitespace-nowrap">{KIND_LABELS[batch.kind]}</td>
                    <td class="px-3 py-2.5 font-mono whitespace-nowrap text-muted"
                        >{when(batch.created_at)}
                        <span class="font-sans">· {callsigns[batch.created_by] ?? '—'}</span></td
                    >
                    <td class="px-3 py-2.5 text-right font-mono">{batch.qso_count}</td>
                    {#if onapply}
                        <td class="px-3 py-2.5 whitespace-nowrap">
                            {#if recordOnly(batch.kind)}
                                <span class="text-muted">Record only</span>
                            {:else if results[batch.id]}
                                <span class="text-ok">Applied · {results[batch.id]}</span>
                            {:else if batch.applied_at}
                                <span class="text-ok">Applied {when(batch.applied_at)}</span>
                            {:else}
                                <button
                                    type="button"
                                    disabled={applying !== undefined}
                                    class="h-[30px] rounded-md border border-accent px-3 text-[13px] font-semibold text-accent disabled:opacity-60"
                                    onclick={() => onapply(batch)}
                                    >{applying === batch.id
                                        ? 'Marking…'
                                        : `Mark ${batch.qso_count} as sent`}</button
                                >
                            {/if}
                        </td>
                    {/if}
                </tr>
            {:else}
                <tr>
                    <td colspan="4" class="px-3 py-4 text-muted">No batches yet.</td>
                </tr>
            {/each}
        </tbody>
    </table>
</div>
