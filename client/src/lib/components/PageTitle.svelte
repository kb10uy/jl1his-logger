<script lang="ts">
    import { m } from '#lib/paraglide/messages.js';

    let { title }: { title?: string } = $props();
</script>

<svelte:head>
    <title>{title ?? m.app_name()}</title>
</svelte:head>
