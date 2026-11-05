<script lang="ts">
    import type { Batch } from '#lib/api/types.ts';
    import { m } from '#lib/paraglide/messages.js';
    import { batchState } from '#lib/transfer/transfer.ts';

    let {
        batch,
        result,
        working,
        onapply,
        ondismiss,
        onrestore
    }: {
        batch: Batch;
        result?: string;
        working?: string;
        onapply: (batch: Batch) => void;
        ondismiss: (batch: Batch) => void;
        onrestore: (batch: Batch) => void;
    } = $props();

    const when = (at: string) => at.replace('T', ' ').slice(0, 16);
    const state = $derived(batchState(batch));
    const busy = $derived(working !== undefined);
</script>

{#if state === 'record'}
    <span class="text-muted">{m.transfer_batch_record_only()}</span>
{:else if result}
    <span class="text-ok">{m.transfer_batch_applied_result({ result })}</span>
{:else if state === 'applied' && batch.applied_at}
    <span class="text-ok">{m.transfer_batch_applied_at({ at: when(batch.applied_at) })}</span>
{:else if state === 'dismissed' && batch.dismissed_at}
    <span class="text-muted">{m.transfer_batch_dismissed_at({ at: when(batch.dismissed_at) })}</span
    >
    <button
        type="button"
        disabled={busy}
        class="ml-2 text-sm text-accent underline disabled:opacity-60"
        onclick={() => onrestore(batch)}>{m.transfer_batch_restore()}</button
    >
{:else}
    <button
        type="button"
        disabled={busy}
        class="h-7 rounded-md border border-accent px-3 text-sm font-semibold text-accent disabled:opacity-60"
        onclick={() => onapply(batch)}
        >{working === batch.id
            ? m.transfer_batch_marking()
            : m.transfer_batch_mark_sent({ count: batch.qso_count })}</button
    >
    <button
        type="button"
        disabled={busy}
        class="ml-1 h-7 rounded-md border border-border px-3 text-sm text-muted disabled:opacity-60"
        onclick={() => ondismiss(batch)}>{m.transfer_batch_dismiss()}</button
    >
{/if}
