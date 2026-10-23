<script lang="ts">
    let { title }: { title?: string } = $props();
</script>

<svelte:head>
    <title>{title ?? 'JL1HIS Logger'}</title>
</svelte:head>
