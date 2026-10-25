<script lang="ts">
    import type { Snippet } from 'svelte';
    import { page } from '$app/state';
    import StationsScreen from '#lib/components/stations/StationsScreen.svelte';

    let { children }: { children: Snippet } = $props();
</script>

<StationsScreen selected={page.params.station} />
{@render children()}
