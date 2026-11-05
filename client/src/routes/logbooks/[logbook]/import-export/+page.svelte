<script lang="ts">
    import { onMount } from 'svelte';
    import { goto } from '$app/navigation';
    import { useLogbook } from '#lib/state/logbook.svelte.ts';

    const logbook = useLogbook();

    onMount(() => {
        if (window.matchMedia('(min-width: 48rem)').matches) {
            goto(`/logbooks/${logbook.logbook.id}/import-export/export`, { replace: true });
        }
    });
</script>
