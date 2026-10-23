<script lang="ts">
    import AppShell from '#lib/components/AppShell.svelte';
    import SettingsFrame from '#lib/components/SettingsFrame.svelte';
    import { lastLogbook, session } from '#lib/state/session.svelte.ts';

    let { children } = $props();

    const logbookId = $derived(
        session.logbook(lastLogbook() ?? '')?.id ?? session.logbooks[0]?.id ?? null
    );
</script>

<AppShell {logbookId}>
    <SettingsFrame {logbookId}>
        {@render children()}
    </SettingsFrame>
</AppShell>
