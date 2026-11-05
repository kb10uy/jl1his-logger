<script lang="ts">
    import { onMount } from 'svelte';
    import { page } from '$app/state';
    import type { Batch } from '#lib/api/types.ts';
    import { sectionDetail } from '#lib/components/SectionFrame.svelte';
    import ExportDetail from '#lib/components/transfer/ExportDetail.svelte';
    import { m } from '#lib/paraglide/messages.js';
    import { useLogbook } from '#lib/state/logbook.svelte.ts';
    import { transferState } from '#lib/state/transfer.svelte.ts';
    import { type ExportRun, KIND_LABELS, exportRuns } from '#lib/transfer/transfer.ts';

    const logbook = useLogbook();
    const batches = transferState(logbook).batches;
    const detail = sectionDetail();
    const exportId = $derived(page.params.export ?? '');
    const run = $derived(exportRuns(batches.items).find((r) => r.id === exportId));

    let loaded = $state(false);

    onMount(async () => {
        await batches.load();
        loaded = true;
    });

    $effect(() => {
        detail.name =
            run === undefined
                ? undefined
                : `${KIND_LABELS[run.kind]()} · ${run.created_at.replace('T', ' ').slice(0, 16)}`;
    });

    async function apply(batch: Batch) {
        await batches.apply(batch);
    }

    async function applyAll(run: ExportRun) {
        await batches.applyExport(run.id);
    }
</script>

{#if run}
    <ExportDetail
        {run}
        callsigns={batches.callsigns}
        results={batches.results}
        working={batches.working}
        onapply={apply}
        ondismiss={(batch) => batches.dismiss(batch)}
        onrestore={(batch) => batches.restore(batch)}
        onapplyall={applyAll}
        ondismissall={(run) => batches.dismissExport(run.id)}
    />
{:else if loaded}
    <span class="text-sm text-muted">{m.transfer_export_missing()}</span>
{/if}
