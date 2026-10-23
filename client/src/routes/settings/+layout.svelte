<script lang="ts">
    import AppShell from '#lib/components/AppShell.svelte';
    import { lastLogbook, session } from '#lib/state/session.svelte.ts';

    let { children } = $props();

    const logbookId = $derived(
        session.logbook(lastLogbook() ?? '')?.id ?? session.logbooks[0]?.id ?? null
    );
</script>

<AppShell {logbookId}>
    {@render children()}
</AppShell>
