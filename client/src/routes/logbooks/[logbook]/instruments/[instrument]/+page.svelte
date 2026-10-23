<script lang="ts">
    import { page } from '$app/state';
    import InstrumentsScreen from '#lib/components/instruments/InstrumentsScreen.svelte';
</script>

<InstrumentsScreen selected={page.params.instrument} />
