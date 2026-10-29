<script lang="ts">
    import type { MergeReport, NewReport } from '#lib/api/types.ts';
    import Button from '#lib/components/ui/Button.svelte';
    import { download } from '#lib/download.ts';
    import { m } from '#lib/paraglide/messages.js';
    import { fileName, rejectCount } from '#lib/transfer/transfer.ts';

    let {
        report,
        logbookName,
        done,
        label,
        disabled,
        onapply
    }: {
        report: NewReport | MergeReport;
        logbookName: string;
        done: string | undefined;
        label: string;
        disabled: boolean;
        onapply: () => void;
    } = $props();
</script>

<div class="flex flex-wrap items-center gap-2">
    {#if report.rejects}
        <Button
            variant="link"
            onclick={() =>
                download(
                    fileName(logbookName, 'adif', new Date(), '-rejects'),
                    report.rejects ?? ''
                )}>{m.transfer_download_rejects({ count: rejectCount(report) })}</Button
        >
    {/if}
    <div class="flex-1"></div>
    {#if done !== undefined}
        <span class="flex h-10 items-center rounded-md bg-ok-bg px-4 font-bold text-ok">{done}</span
        >
    {:else}
        <Button variant="primary" size="lg" {disabled} onclick={onapply}>{label}</Button>
    {/if}
</div>
