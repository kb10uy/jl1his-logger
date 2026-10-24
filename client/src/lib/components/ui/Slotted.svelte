<script lang="ts">
    import type { Snippet } from 'svelte';
    import { slots } from '#lib/i18n.ts';

    let { text, children }: { text: string; children: Snippet } = $props();

    const parts = $derived(slots(text));
</script>

{#each parts as part, i (i)}{#if i > 0}{@render children()}{/if}{part}{/each}
