<script lang="ts">
    import type { Batch } from '#lib/api/types.ts';
    import { m } from '#lib/paraglide/messages.js';
    import { KIND_LABELS, type ExportRun, batchState, recordOnly } from '#lib/transfer/transfer.ts';

    let {
        run,
        callsigns,
        results = {},
        working,
        onapply,
        ondismiss,
        onrestore,
        onapplyall,
        ondismissall
    }: {
        run: ExportRun;
        callsigns: Record<string, string>;
        results?: Record<string, string>;
        working?: string;
        onapply: (batch: Batch) => void;
        ondismiss: (batch: Batch) => void;
        onrestore: (batch: Batch) => void;
        onapplyall: (run: ExportRun) => void;
        ondismissall: (run: ExportRun) => void;
    } = $props();

    const when = (at: string) => at.replace('T', ' ').slice(0, 16);
    const busy = $derived(working !== undefined);
</script>

<div class="flex flex-col gap-3 rounded-lg border border-border bg-surface p-3">
    <div class="flex flex-wrap items-baseline gap-x-2 gap-y-1">
        <span class="text-base font-semibold">{KIND_LABELS[run.kind]()}</span>
        <span class="font-mono text-2xs text-muted">{when(run.created_at)}</span>
        <span class="text-2xs text-muted">· {callsigns[run.created_by] ?? '—'}</span>
        <span class="text-2xs text-muted"
            >· {m.transfer_export_summary({ files: run.batches.length, qsos: run.qsos })}</span
        >
    </div>
    {#if !recordOnly(run.kind) && run.batches.length > 1 && run.pending > 0}
        <div class="flex flex-wrap gap-2">
            <button
                type="button"
                disabled={busy}
                class="h-8 rounded-md border border-accent px-3 text-sm font-semibold text-accent disabled:opacity-60"
                onclick={() => onapplyall(run)}
                >{working === run.id
                    ? m.transfer_batch_marking()
                    : m.transfer_export_mark_all({ count: run.pending })}</button
            >
            <button
                type="button"
                disabled={busy}
                class="h-8 rounded-md border border-border px-3 text-sm text-muted disabled:opacity-60"
                onclick={() => ondismissall(run)}>{m.transfer_export_dismiss_all()}</button
            >
        </div>
    {/if}
    <div class="overflow-x-auto">
        <table class="w-full border-collapse text-sm">
            <thead>
                <tr class="text-left text-2xs text-muted">
                    <th scope="col" class="border-b border-border px-2 py-1.5 font-medium"
                        >{m.transfer_batch_file()}</th
                    >
                    <th
                        scope="col"
                        class="border-b border-border px-2 py-1.5 text-right font-medium"
                        >{m.transfer_batch_qsos()}</th
                    >
                    <th scope="col" class="border-b border-border px-2 py-1.5 font-medium"
                        >{m.transfer_batch_state()}</th
                    >
                </tr>
            </thead>
            <tbody>
                {#each run.batches as batch (batch.id)}
                    {@const state = batchState(batch)}
                    <tr class="border-b border-surface-2 last:border-b-0">
                        <td class="px-2 py-1.5 font-mono whitespace-nowrap">{batch.label ?? '—'}</td
                        >
                        <td class="px-2 py-1.5 text-right font-mono">{batch.qso_count}</td>
                        <td class="px-2 py-1.5 whitespace-nowrap">
                            {#if state === 'record'}
                                <span class="text-muted">{m.transfer_batch_record_only()}</span>
                            {:else if results[batch.id]}
                                <span class="text-ok"
                                    >{m.transfer_batch_applied_result({
                                        result: results[batch.id]
                                    })}</span
                                >
                            {:else if state === 'applied' && batch.applied_at}
                                <span class="text-ok"
                                    >{m.transfer_batch_applied_at({
                                        at: when(batch.applied_at)
                                    })}</span
                                >
                            {:else if state === 'dismissed' && batch.dismissed_at}
                                <span class="text-muted"
                                    >{m.transfer_batch_dismissed_at({
                                        at: when(batch.dismissed_at)
                                    })}</span
                                >
                                <button
                                    type="button"
                                    disabled={busy}
                                    class="ml-2 text-sm text-accent underline disabled:opacity-60"
                                    onclick={() => onrestore(batch)}
                                    >{m.transfer_batch_restore()}</button
                                >
                            {:else}
                                <button
                                    type="button"
                                    disabled={busy}
                                    class="h-7 rounded-md border border-accent px-3 text-sm font-semibold text-accent disabled:opacity-60"
                                    onclick={() => onapply(batch)}
                                    >{working === batch.id
                                        ? m.transfer_batch_marking()
                                        : m.transfer_batch_mark_sent({
                                              count: batch.qso_count
                                          })}</button
                                >
                                <button
                                    type="button"
                                    disabled={busy}
                                    class="ml-1 h-7 rounded-md border border-border px-3 text-sm text-muted disabled:opacity-60"
                                    onclick={() => ondismiss(batch)}
                                    >{m.transfer_batch_dismiss()}</button
                                >
                            {/if}
                        </td>
                    </tr>
                {/each}
            </tbody>
        </table>
    </div>
</div>
