<script lang="ts">
    import Button from '#lib/components/ui/Button.svelte';
    import Notice from '#lib/components/ui/Notice.svelte';
    import { m } from '#lib/paraglide/messages.js';
    import { useLogbook } from '#lib/state/logbook.svelte.ts';

    const logbook = useLogbook();
</script>

{#if logbook.loadError !== undefined}
    <Notice tone="error">
        {m.common_load_failed({ error: logbook.loadError })}
        <Button size="sm" onclick={() => logbook.load()}>{m.common_try_again()}</Button>
    </Notice>
{/if}
