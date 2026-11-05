<script lang="ts">
    import { onMount } from 'svelte';
    import type { Batch } from '#lib/api/types.ts';
    import ExportList from '#lib/components/transfer/ExportList.svelte';
    import { m } from '#lib/paraglide/messages.js';
    import { useLogbook } from '#lib/state/logbook.svelte.ts';
    import { transferState } from '#lib/state/transfer.svelte.ts';
    import { exportRuns, partitionRuns } from '#lib/transfer/transfer.ts';

    const logbook = useLogbook();
    const batches = transferState(logbook).batches;
    const lists = $derived(partitionRuns(exportRuns(batches.items)));
    const empty = $derived(batches.items.length === 0);

    const actions = {
        onapply: (batch: Batch) => batches.apply(batch),
        ondismiss: (batch: Batch) => batches.dismiss(batch),
        onrestore: (batch: Batch) => batches.restore(batch),
        onapplyall: (run: { id: string }) => batches.applyExport(run.id),
        ondismissall: (run: { id: string }) => batches.dismissExport(run.id)
    };

    onMount(() => batches.load());
</script>

<ExportList
    title={m.transfer_list_pending()}
    empty={empty ? m.transfer_batch_none() : m.transfer_pending_none()}
    runs={lists.pending}
    callsigns={batches.callsigns}
    results={batches.results}
    working={batches.working}
    {...actions}
/>
<ExportList
    title={m.transfer_list_finished()}
    runs={lists.finished}
    callsigns={batches.callsigns}
    results={batches.results}
    working={batches.working}
    {...actions}
/>
