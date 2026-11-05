<script lang="ts">
    import type { Batch, BatchPage } from '#lib/api/types.ts';
    import BatchState from '#lib/components/transfer/BatchState.svelte';
    import Icon from '#lib/components/ui/Icon.svelte';
    import { formatNumber } from '#lib/i18n.ts';
    import { m } from '#lib/paraglide/messages.js';
    import { KIND_LABELS, type ExportRun, exportRuns, recordOnly } from '#lib/transfer/transfer.ts';

    let {
        title,
        empty,
        list,
        callsigns,
        results,
        working,
        onmore,
        onapply,
        ondismiss,
        onrestore,
        onapplyall,
        ondismissall
    }: {
        title: string;
        empty?: string;
        list: BatchPage;
        callsigns: Record<string, string>;
        results: Record<string, string>;
        working?: string;
        onmore: () => void;
        onapply: (batch: Batch) => void;
        ondismiss: (batch: Batch) => void;
        onrestore: (batch: Batch) => void;
        onapplyall: (run: ExportRun) => void;
        ondismissall: (run: ExportRun) => void;
    } = $props();

    const runs = $derived(exportRuns(list.batches));
    const when = (at: string) => at.replace('T', ' ').slice(0, 16);
    const busy = $derived(working !== undefined);
    const actions = $derived({ working, onapply, ondismiss, onrestore });
</script>

{#snippet head(run: ExportRun)}
    <span class="flex min-w-0 flex-1 flex-col gap-0.5">
        <span class="flex flex-wrap items-center gap-x-2">
            <span class="font-medium">{KIND_LABELS[run.kind]()}</span>
            {#if run.batches.length > 1}
                <span class="text-2xs text-muted"
                    >{m.transfer_export_files({ count: run.batches.length })}</span
                >
            {:else if run.batches[0].label}
                <span class="font-mono text-2xs text-muted">{run.batches[0].label}</span>
            {/if}
        </span>
        <span class="flex flex-wrap items-center gap-x-2 text-2xs text-muted">
            <span class="font-mono">{when(run.created_at)}</span>
            <span>· {callsigns[run.created_by] ?? '—'}</span>
            <span>· {m.transfer_export_qsos({ count: run.qsos })}</span>
        </span>
    </span>
{/snippet}

{#if runs.length > 0 || empty}
    <section aria-label={title} class="flex flex-col gap-1">
        <h2 class="eyebrow">{title} · {formatNumber(list.total)}</h2>
        <ul class="flex flex-col overflow-hidden rounded-lg border border-border bg-surface">
            {#each runs as run (run.id)}
                <li class="border-b border-surface-2 text-sm last:border-b-0">
                    {#if run.batches.length === 1}
                        <div class="flex min-h-12 flex-wrap items-center gap-x-3 gap-y-1 px-3 py-2">
                            {@render head(run)}
                            <span class="whitespace-nowrap">
                                <BatchState
                                    batch={run.batches[0]}
                                    result={results[run.batches[0].id]}
                                    {...actions}
                                />
                            </span>
                        </div>
                    {:else}
                        <details name="export-run" class="group">
                            <summary
                                class="flex min-h-12 cursor-pointer list-none items-center gap-x-3 px-3 py-2 hover:bg-surface-2/60 [&::-webkit-details-marker]:hidden"
                            >
                                {@render head(run)}
                                {#if recordOnly(run.kind)}
                                    <span class="text-2xs text-muted"
                                        >{m.transfer_batch_record_only()}</span
                                    >
                                {:else if run.pending > 0}
                                    <span class="text-2xs text-warn"
                                        >{m.transfer_export_pending({ count: run.pending })}</span
                                    >
                                {:else}
                                    <span class="text-2xs text-ok">{m.transfer_export_done()}</span>
                                {/if}
                                <Icon
                                    name="chevron"
                                    size={16}
                                    class="text-muted group-open:rotate-180"
                                />
                            </summary>
                            <div class="flex flex-col gap-2 px-3 pb-3">
                                {#if run.pending > 0}
                                    <div class="flex flex-wrap gap-2">
                                        <button
                                            type="button"
                                            disabled={busy}
                                            class="h-8 rounded-md border border-accent px-3 text-sm font-semibold text-accent disabled:opacity-60"
                                            onclick={() => onapplyall(run)}
                                            >{working === run.id
                                                ? m.transfer_batch_marking()
                                                : m.transfer_export_mark_all({
                                                      count: run.pending
                                                  })}</button
                                        >
                                        <button
                                            type="button"
                                            disabled={busy}
                                            class="h-8 rounded-md border border-border px-3 text-sm text-muted disabled:opacity-60"
                                            onclick={() => ondismissall(run)}
                                            >{m.transfer_export_dismiss_all()}</button
                                        >
                                    </div>
                                {/if}
                                <div class="overflow-x-auto">
                                    <table class="w-full border-collapse text-sm">
                                        <thead>
                                            <tr class="text-left text-2xs text-muted">
                                                <th
                                                    scope="col"
                                                    class="border-b border-border px-2 py-1.5 font-medium"
                                                    >{m.transfer_batch_file()}</th
                                                >
                                                <th
                                                    scope="col"
                                                    class="border-b border-border px-2 py-1.5 text-right font-medium"
                                                    >{m.transfer_batch_qsos()}</th
                                                >
                                                <th
                                                    scope="col"
                                                    class="border-b border-border px-2 py-1.5 font-medium"
                                                    >{m.transfer_batch_state()}</th
                                                >
                                            </tr>
                                        </thead>
                                        <tbody>
                                            {#each run.batches as batch (batch.id)}
                                                <tr
                                                    class="border-b border-surface-2 last:border-b-0"
                                                >
                                                    <td
                                                        class="px-2 py-1.5 font-mono whitespace-nowrap"
                                                        >{batch.label ?? '—'}</td
                                                    >
                                                    <td class="px-2 py-1.5 text-right font-mono"
                                                        >{batch.qso_count}</td
                                                    >
                                                    <td class="px-2 py-1.5 whitespace-nowrap">
                                                        <BatchState
                                                            {batch}
                                                            result={results[batch.id]}
                                                            {...actions}
                                                        />
                                                    </td>
                                                </tr>
                                            {/each}
                                        </tbody>
                                    </table>
                                </div>
                            </div>
                        </details>
                    {/if}
                </li>
            {:else}
                <li class="px-3 py-4 text-sm text-muted">{empty}</li>
            {/each}
        </ul>
        {#if list.next !== null}
            <button
                type="button"
                disabled={busy}
                class="self-start px-1 text-sm text-info disabled:opacity-60"
                onclick={onmore}>{m.transfer_show_more({ count: list.total - runs.length })}</button
            >
        {/if}
    </section>
{/if}
