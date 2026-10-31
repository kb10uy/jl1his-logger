<script lang="ts">
    import type { Snippet } from 'svelte';

    let {
        label,
        adif,
        for: id,
        class: className = '',
        children
    }: { label: string; adif?: string; for?: string; class?: string; children: Snippet } = $props();
</script>

<div class={['flex min-w-0 flex-col gap-1', className]}>
    <label for={id} class="flex flex-col text-2xs text-muted">
        <span>{label}</span>
        {#if adif}
            <span class="font-mono text-2xs">{adif}</span>
        {/if}
    </label>
    {@render children()}
</div>
