<script lang="ts">
    import { m } from '#lib/paraglide/messages.js';
    import { KIND_LABELS, type ExportRun, recordOnly } from '#lib/transfer/transfer.ts';

    let {
        runs,
        callsigns,
        href
    }: {
        runs: ExportRun[];
        callsigns: Record<string, string>;
        href: (id: string) => string;
    } = $props();

    const when = (at: string) => at.replace('T', ' ').slice(0, 16);
</script>

<ul
    aria-label={m.transfer_batches()}
    class="flex flex-col overflow-hidden rounded-lg border border-border bg-surface"
>
    {#each runs as run (run.id)}
        <li class="border-b border-surface-2 last:border-b-0">
            <a
                href={href(run.id)}
                class="flex min-h-12 flex-col justify-center gap-0.5 px-3 py-2 text-sm text-text no-underline hover:bg-surface-2/60"
            >
                <span class="flex items-center gap-2">
                    <span class="font-medium">{KIND_LABELS[run.kind]()}</span>
                    {#if run.batches.length > 1}
                        <span class="text-2xs text-muted"
                            >{m.transfer_export_files({ count: run.batches.length })}</span
                        >
                    {/if}
                    <span class="ml-auto font-mono text-2xs text-muted">{run.qsos}</span>
                </span>
                <span class="flex items-center gap-2 text-2xs text-muted">
                    <span class="font-mono">{when(run.created_at)}</span>
                    <span>· {callsigns[run.created_by] ?? '—'}</span>
                    {#if recordOnly(run.kind)}
                        <span class="ml-auto">{m.transfer_batch_record_only()}</span>
                    {:else if run.pending > 0}
                        <span class="ml-auto text-warn"
                            >{m.transfer_export_pending({ count: run.pending })}</span
                        >
                    {:else}
                        <span class="ml-auto text-ok">{m.transfer_export_done()}</span>
                    {/if}
                </span>
            </a>
        </li>
    {:else}
        <li class="px-3 py-4 text-sm text-muted">{m.transfer_batch_none()}</li>
    {/each}
</ul>
