<script lang="ts">
    import type { Snippet } from 'svelte';

    let {
        title,
        padded = true,
        form = false,
        onsubmit,
        children
    }: {
        title: string;
        padded?: boolean;
        form?: boolean;
        onsubmit?: (event: SubmitEvent) => void;
        children: Snippet;
    } = $props();
</script>

<section aria-label={title} class="flex flex-col gap-2">
    <h2 class="eyebrow">{title}</h2>
    <svelte:element
        this={form ? 'form' : 'div'}
        {onsubmit}
        class={[
            'rounded-lg border border-border bg-surface',
            padded ? 'flex flex-col gap-3 p-4' : 'overflow-x-auto'
        ]}
    >
        {@render children()}
    </svelte:element>
</section>
