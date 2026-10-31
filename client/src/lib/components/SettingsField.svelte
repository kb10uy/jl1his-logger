<script lang="ts">
    import type { Snippet } from 'svelte';

    let {
        label,
        for: id,
        hint,
        children
    }: { label: string; for?: string; hint?: string; children: Snippet } = $props();
</script>

<svelte:element
    this={id === undefined ? 'span' : 'label'}
    for={id}
    class="text-2xs text-muted md:pt-3">{label}</svelte:element
>
<div class="flex min-w-0 flex-col gap-1">
    {@render children()}
    {#if hint}
        <span class="text-2xs text-muted">{hint}</span>
    {/if}
</div>
