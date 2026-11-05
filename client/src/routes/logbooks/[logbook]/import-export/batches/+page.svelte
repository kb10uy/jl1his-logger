<script lang="ts">
    import { onMount } from 'svelte';
    import ExportList from '#lib/components/transfer/ExportList.svelte';
    import { useLogbook } from '#lib/state/logbook.svelte.ts';
    import { transferState } from '#lib/state/transfer.svelte.ts';
    import { exportRuns } from '#lib/transfer/transfer.ts';

    const logbook = useLogbook();
    const batches = transferState(logbook).batches;
    const root = `/logbooks/${logbook.logbook.id}/import-export/batches`;
    const runs = $derived(exportRuns(batches.items));

    onMount(() => batches.load());
</script>

<ExportList {runs} callsigns={batches.callsigns} href={(id) => `${root}/${id}`} />
