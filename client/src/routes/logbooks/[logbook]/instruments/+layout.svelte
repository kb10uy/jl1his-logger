<script lang="ts">
    import type { Snippet } from 'svelte';
    import { page } from '$app/state';
    import InstrumentsScreen from '#lib/components/instruments/InstrumentsScreen.svelte';

    let { children }: { children: Snippet } = $props();
</script>

<InstrumentsScreen selected={page.params.instrument} />
{@render children()}
