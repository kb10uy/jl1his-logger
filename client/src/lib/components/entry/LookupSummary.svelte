<script lang="ts">
    import type { Lookup } from '#lib/api/types.ts';
    import { lookupLine } from '#lib/entry/lookup.ts';
    import { m } from '#lib/paraglide/messages.js';

    let { lookup, showCall = false }: { lookup: Lookup; showCall?: boolean } = $props();
</script>

<div class="flex flex-wrap items-center gap-x-2.5 gap-y-1.5 text-[13px]">
    {#if showCall}
        <span class="font-mono font-semibold">{lookup.call}</span>
    {/if}
    <span class="text-muted">{lookupLine(lookup)}</span>
    {#if lookup.previous.count > 0}
        <span class="rounded-full bg-info-bg px-2 py-px text-xs text-info"
            >{m.entry_worked_before({ count: lookup.previous.count })}</span
        >
    {/if}
</div>
