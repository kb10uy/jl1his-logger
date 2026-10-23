<script lang="ts">
    import { untrack, type Snippet } from 'svelte';
    import type { Logbook } from '#lib/api/types.ts';
    import AppShell from '#lib/components/AppShell.svelte';
    import { LogbookState, provideLogbook } from '#lib/state/logbook.svelte.ts';
    import { outbox } from '#lib/state/outbox.svelte.ts';
    import { rememberLogbook } from '#lib/state/session.svelte.ts';

    let { logbook, children }: { logbook: Logbook; children: Snippet } = $props();

    const state = new LogbookState(untrack(() => logbook));
    provideLogbook(state);

    $effect(() => {
        state.start();
        rememberLogbook(logbook.id);
        outbox.show(logbook.id);
        return () => state.stop();
    });
</script>

<AppShell logbookId={logbook.id} status={state.status} lastUpdatedSeq={state.lastUpdatedSeq}>
    {@render children()}
</AppShell>
