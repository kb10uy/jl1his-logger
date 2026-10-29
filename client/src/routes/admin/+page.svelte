<script lang="ts">
    import { onMount } from 'svelte';
    import { goto } from '$app/navigation';

    onMount(() => {
        if (window.matchMedia('(min-width: 48rem)').matches) {
            goto('/admin/server', { replace: true });
        }
    });
</script>
