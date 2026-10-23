<script lang="ts">
    import { page } from '$app/state';
    import StationsScreen from '#lib/components/stations/StationsScreen.svelte';
</script>

<StationsScreen selected={page.params.station} />
